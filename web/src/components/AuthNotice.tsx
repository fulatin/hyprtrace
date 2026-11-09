import { useEffect, useState } from 'react';
import { KeyRound, X } from 'lucide-react';
import { AUTH_REJECTED_EVENT } from '../lib/auth';

/**
 * Persistent banner shown after the server rejected the stored API token.
 *
 * The API client has already cleared the token (otherwise every request would
 * keep failing with a secret that no longer works); this tells the user how to
 * get a working one back, which is the missing half of issue #27's acceptance:
 * "收到 401 后 localStorage 中的 token 被清除，UI 提示重新输入".
 */
export default function AuthNotice() {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const show = () => setVisible(true);
    window.addEventListener(AUTH_REJECTED_EVENT, show);
    return () => window.removeEventListener(AUTH_REJECTED_EVENT, show);
  }, []);

  if (!visible) return null;

  return (
    <div
      role="alert"
      className="flex items-start gap-3 border-b border-warn/40 bg-warn/10 px-6 py-2.5 text-xs text-fg"
    >
      <KeyRound size={14} className="mt-0.5 shrink-0 text-warn" aria-hidden="true" />
      <p className="min-w-0 flex-1 leading-relaxed">
        The server rejected the saved API token, so it has been cleared. Reopen this page once
        with the current token as{' '}
        <code className="rounded bg-surface-2 px-1 py-0.5 text-[11px]">
          ?token=&lt;your-token&gt;
        </code>{' '}
        — it will be stored again and this notice disappears.
      </p>
      <button
        type="button"
        onClick={() => setVisible(false)}
        className="btn btn-ghost shrink-0 px-1.5 py-1"
        aria-label="Dismiss authentication notice"
      >
        <X size={13} />
      </button>
    </div>
  );
}
