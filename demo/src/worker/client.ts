// The app's side of the worker: typed calls, one worker at a time, and cancellation.
//
// A detector run cannot be interrupted from inside (WASM does not yield), so cancelling means
// terminating the worker and starting a fresh one on the next call. The fresh one has not seen
// the image yet, so `detect` sends it again whenever the worker's image is not the one asked for.

import type { ImageRecord } from "../lib/image";
import type {
  DetectInput,
  DetectOutcome,
  RenderRequest,
  RenderedTarget,
  Defaults,
  WorkerMethod,
  WorkerMethods,
  WorkerRequest,
  WorkerResponse,
} from "./protocol";

export class CancelledError extends Error {
  constructor() {
    super("cancelled");
    this.name = "CancelledError";
  }
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

class WorkerClient {
  #worker: Worker | null = null;
  #pending = new Map<number, Pending>();
  #nextId = 1;
  #preparedKey: number | null = null;

  #spawn(): Worker {
    if (this.#worker !== null) return this.#worker;
    const worker = new Worker(new URL("./detect.worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const response = event.data;
      const pending = this.#pending.get(response.id);
      if (pending === undefined) return;
      this.#pending.delete(response.id);
      if (response.ok) pending.resolve(response.value);
      else pending.reject(new Error(response.error));
    };
    worker.onerror = (event) => {
      this.#reject(new Error(event.message || "The worker crashed."));
      this.cancel();
    };
    this.#worker = worker;
    return worker;
  }

  #reject(error: Error): void {
    const pending = [...this.#pending.values()];
    this.#pending.clear();
    for (const entry of pending) entry.reject(error);
  }

  #call<M extends WorkerMethod>(
    method: M,
    args: WorkerMethods[M]["args"],
    transfer: Transferable[] = [],
  ): Promise<WorkerMethods[M]["result"]> {
    const worker = this.#spawn();
    const id = this.#nextId++;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      const request: WorkerRequest<M> = { id, method, args };
      worker.postMessage(request, transfer);
    });
  }

  /** Stop whatever the worker is doing. Pending calls reject with `CancelledError`. */
  cancel(): void {
    this.#worker?.terminate();
    this.#worker = null;
    this.#preparedKey = null;
    this.#reject(new CancelledError());
  }

  defaults(): Promise<Defaults> {
    return this.#call("defaults", []);
  }

  async detect(image: ImageRecord, input: DetectInput): Promise<DetectOutcome> {
    if (this.#preparedKey !== image.key) {
      // A copy: the worker takes the buffer, and the app keeps the pixels for the next worker.
      const rgba = image.rgba.slice().buffer;
      await this.#call("prepare", [{ key: image.key, rgba, width: image.width, height: image.height }], [rgba]);
      this.#preparedKey = image.key;
    }
    return this.#call("detect", [{ ...input, imageKey: image.key }]);
  }

  render(request: RenderRequest): Promise<RenderedTarget> {
    return this.#call("render", [request]);
  }
}

export const worker = new WorkerClient();
