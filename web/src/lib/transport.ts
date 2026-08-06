import { HttpChatTransport } from 'ai';
import type { UIMessage, UIMessageChunk } from 'ai';

/**
 * Transport for the HyprTrace agent endpoint (`/api/ai/chat/agent`).
 *
 * The backend streams NDJSON events, one JSON object per line:
 *   {"type":"text","delta":"..."}
 *   {"type":"tool_call","id":"...","name":"...","args":{...}}
 *   {"type":"tool_result","id":"...","name":"...","ok":bool,"result":...}
 *   {"type":"done"}
 *   {"type":"error","message":"..."}
 *
 * This transport converts those events into AI SDK UIMessageChunk objects so
 * `useChat` renders both streamed text and tool invocations.
 */
export class NdjsonChatTransport<
  UI_MESSAGE extends UIMessage,
> extends HttpChatTransport<UI_MESSAGE> {
  protected processResponseStream(
    stream: ReadableStream<Uint8Array>,
  ): ReadableStream<UIMessageChunk> {
    let textId: string | null = null;
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

    const finish = (
      controller: TransformStreamDefaultController<UIMessageChunk>,
    ) => {
      if (finished) return;
      finished = true;
      closeText(controller);
      controller.enqueue({ type: 'finish-step' });
      controller.enqueue({ type: 'finish' });
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
          case 'text': {
            const delta = String(event.delta ?? '');
            if (!delta) break;
            if (textId === null) {
              counter += 1;
              textId = `text-${counter}`;
              controller.enqueue({ type: 'text-start', id: textId });
            }
            controller.enqueue({ type: 'text-delta', id: textId, delta });
            break;
          }

          case 'tool_call': {
            closeText(controller);
            controller.enqueue({
              type: 'tool-input-available',
              toolCallId: String(event.id ?? `call_${counter}`),
              toolName: String(event.name ?? 'unknown'),
              input: event.args ?? {},
            });
            break;
          }

          case 'tool_result': {
            closeText(controller);
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

          case 'done': {
            finish(controller);
            break;
          }

          case 'error': {
            closeText(controller);
            finished = true;
            controller.enqueue({
              type: 'error',
              errorText: String(event.message ?? 'Unknown error'),
            });
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
