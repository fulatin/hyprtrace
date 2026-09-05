import { AlertTriangle } from 'lucide-react';

interface Props {
  message: string;
  onRetry?: () => void;
}

export default function ErrorState({ message, onRetry }: Props) {
  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 min-h-[16rem] flex flex-col items-center justify-center text-center space-y-3">
      <AlertTriangle size={28} className="text-red-400" />
      <div>
        <p className="text-sm font-medium text-red-400">Failed to load data</p>
        <p className="text-xs text-gray-500 mt-1 break-all">{message}</p>
      </div>
      {onRetry && (
        <button
          onClick={onRetry}
          className="bg-cyan-600 hover:bg-cyan-700 text-white rounded-lg px-4 py-1.5 text-sm transition-colors"
        >
          Retry
        </button>
      )}
    </div>
  );
}
