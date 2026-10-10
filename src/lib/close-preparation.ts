let prepare: (() => Promise<void>) | null = null;

export function registerClosePreparation(callback: () => Promise<void>): () => void {
  prepare = callback;
  return () => { if (prepare === callback) prepare = null; };
}

export async function prepareToClose(): Promise<void> {
  await prepare?.();
}
