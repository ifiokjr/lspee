// Example TypeScript file for outline extraction testing

/** Maximum batch size for processing. */
const MAX_BATCH_SIZE = 100;

/** Application version string. */
const VERSION = "1.0.0";

/** Configuration for the processor. */
interface ProcessorConfig {
  /** Number of worker threads. */
  threads: number;
  /** Whether to enable debug logging. */
  verbose: boolean;
  /** Maximum retries on failure. */
  maxRetries?: number;
}

/** A processing result containing output data. */
type ProcessResult<T> = {
  ok: true;
  data: T;
} | {
  ok: false;
  error: string;
};

/** Status of a processing job. */
enum JobStatus {
  Pending = "pending",
  Running = "running",
  Done = "done",
  Failed = "failed",
}

/** Manages a pool of processing workers. */
class WorkerPool {
  private workers: ProcessorConfig[];
  private active: number;

  /** Create a new worker pool with the given configuration. */
  constructor(config: ProcessorConfig) {
    this.workers = [config];
    this.active = 0;
  }

  /** Submit a job to the pool for processing. */
  process<T>(input: T): ProcessResult<T> {
    if (this.active >= this.workers.length) {
      return { ok: false, error: "pool full" };
    }
    this.active++;
    return { ok: true, data: input };
  }

  /** Shut down all workers. */
  shutdown(): void {
    this.active = 0;
  }
}

/** Parse a JSON string into a typed result. */
function parseJson<T>(input: string): ProcessResult<T> {
  try {
    const data = JSON.parse(input) as T;
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

/** Format a result for display. */
const formatResult = <T>(result: ProcessResult<T>): string => {
  if (result.ok) {
    return `OK: ${JSON.stringify(result.data)}`;
  }
  return `ERR: ${result.error}`;
};

export { WorkerPool, parseJson, formatResult, JobStatus };