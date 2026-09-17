import { archiveBatchApi, type ArchiveBatchAction, type ArchiveBatchRequest, type ArchivePreview, type ArchiveJob, type ArchiveBatchExecution } from "$lib/api/archiveApi";

export function createArchiveBatchStore(port = archiveBatchApi, identity: () => string = () => crypto.randomUUID()) {
  let busy = false;
  return {
    pending: port.pending,
    dismiss: port.dismiss,
    prepare(action: ArchiveBatchAction, items: ArchivePreview[]): ArchiveBatchRequest {
      if (!items.length) throw Error("ARCHIVE_EMPTY_SELECTION");
      return { operation: identity(), action, targets: items.map(item => structuredClone(item.expected)) };
    },
    async execute(request: ArchiveBatchRequest, refresh: () => Promise<void>, progress: (job: ArchiveJob) => void): Promise<ArchiveBatchExecution> {
      if (busy) throw Error("ARCHIVE_BUSY");
      busy = true;
      const snapshot = structuredClone(request);
      let job: ArchiveJob | null = null, error: string | null = null, refreshNeeded = false;
      try {
        job = await port.prepare(snapshot);
        progress(structuredClone(job));
        while (job.results.length < job.targets.length) {
          const previous = job.results.length;
          job = await port.run(snapshot.operation);
          progress(structuredClone(job));
          if (job.results.length <= previous) throw Error("ARCHIVE_INVALID_STATE");
        }
      } catch (failure) { error = failure instanceof Error ? failure.message : String(failure); }
      finally {
        // A failed reply can still have committed tasks; refresh independently.
        try { await refresh(); } catch { refreshNeeded = true; }
        busy = false;
      }
      return { job, error, refreshNeeded };
    },
  };
}
