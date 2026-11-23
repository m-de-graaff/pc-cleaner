import { useEffect, useRef, type ReactNode } from "react";

interface Props {
  open: boolean;
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ open, title, children, confirmLabel, onConfirm, onCancel }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      aria-labelledby="confirm-title"
      onCancel={(e) => { e.preventDefault(); onCancel(); }}
      className="m-auto max-w-md rounded-xl border border-zinc-700 bg-zinc-900 p-6 text-zinc-100 backdrop:bg-black/60"
    >
      <h2 id="confirm-title" className="text-lg font-semibold">{title}</h2>
      <div className="mt-2 text-sm text-zinc-300">{children}</div>
      <div className="mt-6 flex justify-end gap-2">
        <button type="button" autoFocus onClick={onCancel}
          className="rounded-lg border border-zinc-700 px-4 py-2 hover:bg-zinc-800 focus-visible:outline-2 focus-visible:outline-sky-400">
          Cancel
        </button>
        <button type="button" onClick={onConfirm}
          className="rounded-lg bg-red-500 px-4 py-2 font-medium text-white hover:bg-red-400 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-red-300">
          {confirmLabel}
        </button>
      </div>
    </dialog>
  );
}
