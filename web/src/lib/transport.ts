import { HttpChatTransport } from 'ai';
import type { HttpChatTransportInitOptions, UIMessage, UIMessageChunk } from 'ai';

/**
 * Transport for the HyprTrace agent endpoint (`/api/ai/chat/agent`).
 *
 * The backend streams NDJSON events, one JSON object per line:
 *   {"type":"reasoning","delta":"..."}                       // thinking trace
 *   {"type":"text","delta":"..."}
 *   {"type":"tool_call","id":"...","name":"...","args":{...}}
 *   {"type":"tool_result","id":"...","name":"...","ok":bool,"result":...}
 *   {"type":"cancelled"}                                     // terminal
 *   {"type":"done"}                                          // terminal
 *   {"type":"error","message":"..."}
 *
 * This transport converts those events into AI SDK UIMessageChunk objects so
 * `useChat` renders streamed reasoning, text and tool invocations.
 *
 * `reasoning` maps to the SDK's reasoning-start/delta/end chunks (the `ai`
 * package in this repo supports them, and `useChat` turns them into
 * `{type:'reasoning'}` message parts). `cancelled` is terminal — the turn was
 * stopped (by the user, by `/api/ai/cancel`, or by the server) and the stream
 * ends right there — so it maps to an `abort` chunk plus the closing brackets
 * and fires `onCancelled` for the caller's "已停止" marker.
 */
export interface NdjsonChatTransportOptions<UI_MESSAGE extends UIMessage>
  extends HttpChatTransportInitOptions<UI_MESSAGE> {
  /** Called once when the backend emits the terminal `cancelled` event. */
  onCancelled?: () => void;
}

export class NdjsonChatTransport<
  UI_MESSAGE extends UIMessage,
> extends HttpChatTransport<UI_MESSAGE> {
  private readonly onCancelled?: () => void;

  constructor(options: NdjsonChatTransportOptions<UI_MESSAGE>) {
    const { onCancelled, ...init } = options;
    super(init);
    this.onCancelled = onCancelled;
  }

  protected processResponseStream(
    stream: ReadableStream<Uint8Array>,
  ): ReadableStream<UIMessageChunk> {
    // One open text part and one open reasoning part at a time; ids are handed
    // out from `counter` so the SDK can track parts across tool calls.
    let textId: string | null = null;
    let reasoningId: string | null = null;
    let counter = 0;
    let finished = false;

    const closeText = (
      controller: TransformStreamDefaultController<UIMessageChunk>,
    ) => {
      if (textId !== null) {
        controller.enqueue({ type: 'text-end', id: textId });
        textId = null;
      }
    };

    const closeReasoning = (
      controller: TransformStreamDefaultController<UIMessageChunk>,
    ) => {
      if (reasoningId !== null) {
        controller.enqueue({ type: 'reasoning-end', id: reasoningId });
        reasoningId = null;
      }
    };

    const closeAll = (
      controller: TransformStreamDefaultController<UIMessageChunk>,
    ) => {
      closeText(controller);
      closeReasoning(controller);
    };

    const finish = (
      controller: TransformStreamDefaultController<UIMessageChunk>,
    ) => {
      if (finished) return;
      finished = true;
      closeAll(controller);
      controller.enqueue({ type: 'finish-step' });
      controller.enqueue({ type: 'finish' });
    };

    const finishCancelled = (
      controller: TransformStreamDefaultController<UIMessageChunk>,
    ) => {
      if (finished) return;
      finished = true;
      closeAll(controller);
      // `abort` is the SDK's terminal "this response was stopped" chunk: it
      // sets `isAbort` on the finish callback without surfacing an error.
      controller.enqueue({ type: 'abort', reason: 'cancelled' });
      controller.enqueue({ type: 'finish-step' });
      controller.enqueue({ type: 'finish' });
      this.onCancelled?.();
    };

    const mapped = new TransformStream<string, UIMessageChunk>({
      start(controller) {
        controller.enqueue({ type: 'start' });
        controller.enqueue({ type: 'start-step' });
      },

      transform(rawLine, controller) {
        const line = rawLine.trim();
        if (!line) return;

        let event: any;
        try {
          event = JSON.parse(line);
        } catch {
          return;
        }

        switch (event.type) {
          case 'reasoning': {
            const delta = String(event.delta ?? '');
            if (!delta) break;
            // Reasoning always starts a fresh part after text: a text block is
            // closed first so the transcript reads "think → answer".
            closeText(controller);
            if (reasoningId === null) {
              counter += 1;
              reasoningId = `reasoning-${counter}`;
              controller.enqueue({ type: 'reasoning-start', id: reasoningId });
            }
            controller.enqueue({
              type: 'reasoning-delta',
              id: reasoningId,
              delta,
            });
            break;
          }

          case 'text': {
            const delta = String(event.delta ?? '');
            if (!delta) break;
            closeReasoning(controller);
            if (textId === null) {
              counter += 1;
              textId = `text-${counter}`;
              controller.enqueue({ type: 'text-start', id: textId });
            }
            controller.enqueue({ type: 'text-delta', id: textId, delta });
            break;
          }

          case 'tool_call': {
            closeAll(controller);
            controller.enqueue({
              type: 'tool-input-available',
              toolCallId: String(event.id ?? `call_${counter}`),
              toolName: String(event.name ?? 'unknown'),
              input: event.args ?? {},
            });
            break;
          }

          case 'tool_result': {
            closeAll(controller);
            if (event.ok) {
              controller.enqueue({
                type: 'tool-output-available',
                toolCallId: String(event.id ?? ''),
                output: event.result ?? null,
              });
            } else {
              controller.enqueue({
                type: 'tool-output-error',
                toolCallId: String(event.id ?? ''),
                errorText: String(event.result ?? 'tool failed'),
              });
            }
            break;
          }

          case 'cancelled': {
            // Terminal by contract: no `done`/`error` follows.
            finishCancelled(controller);
            break;
          }

          case 'done': {
            finish(controller);
            break;
          }

          case 'error': {
            // Terminal too: like `cancelled`, close the open text/reasoning
            // parts and emit the finish brackets so later deltas cannot arrive
            // after the error (the SDK would otherwise log an unfinished
            // response).
            if (finished) break;
            finished = true;
            closeAll(controller);
            controller.enqueue({
              type: 'error',
              errorText: String(event.message ?? 'Unknown error'),
            });
            controller.enqueue({ type: 'finish-step' });
            controller.enqueue({ type: 'finish' });
            break;
          }
        }
      },

      flush(controller) {
        finish(controller);
      },
    });

    const decoded: ReadableStream<string> = stream.pipeThrough(
      new TextDecoderStream() as unknown as ReadableWritablePair<
        string,
        Uint8Array
      >,
    );
    return decoded.pipeThrough(splitLines()).pipeThrough(mapped);
  }
}

/** Split a text stream into individual lines (handles partial lines). */
function splitLines(): TransformStream<string, string> {
  let buffer = '';
  return new TransformStream<string, string>({
    transform(chunk, controller) {
      buffer += chunk;
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const line of lines) {
        controller.enqueue(line);
      }
    },
    flush(controller) {
      if (buffer.length > 0) {
        controller.enqueue(buffer);
        buffer = '';
      }
    },
  });
}
