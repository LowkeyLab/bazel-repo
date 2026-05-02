import { Injectable, InjectionToken, inject } from '@angular/core';

import { Candidate } from '../editor/phrase-token';

export interface ConversionResultMessage {
  readonly requestId: string;
  readonly type: 'conversion-result';
  readonly result: {
    readonly sourcePinyin: string;
    readonly candidates: readonly Candidate[];
  };
}

export interface ConversionErrorMessage {
  readonly requestId: string;
  readonly type: 'conversion-error';
  readonly message: string;
}

type WorkerResponse = ConversionResultMessage | ConversionErrorMessage;

export const CONVERSION_WORKER_FACTORY = new InjectionToken<() => Worker>(
  'CONVERSION_WORKER_FACTORY',
  {
    providedIn: 'root',
    factory: () => () =>
      new Worker(new URL('./conversion.worker', import.meta.url), {
        type: 'module',
      }),
  },
);

@Injectable({ providedIn: 'root' })
export class ConversionWorkerClient {
  private readonly workerFactory = inject(CONVERSION_WORKER_FACTORY);
  private readonly worker: Worker;
  private readonly pending = new Map<
    string,
    {
      resolve: (value: readonly Candidate[]) => void;
      reject: (reason: Error) => void;
    }
  >();

  constructor() {
    this.worker = this.workerFactory();
    this.worker.onmessage = (event: MessageEvent<WorkerResponse>) =>
      this.handleMessage(event.data);
  }

  convertPinyin(
    sourcePinyin: string,
    limit: number,
  ): Promise<readonly Candidate[]> {
    const requestId = crypto.randomUUID();
    const promise = new Promise<readonly Candidate[]>((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
    });

    this.worker.postMessage({
      requestId,
      type: 'convert-pinyin',
      sourcePinyin,
      limit,
    });
    return promise;
  }

  dispose(): void {
    this.worker.terminate();
    this.pending.clear();
  }

  private handleMessage(message: WorkerResponse): void {
    const pending = this.pending.get(message.requestId);
    if (!pending) {
      return;
    }

    this.pending.delete(message.requestId);
    if (message.type === 'conversion-result') {
      pending.resolve(message.result.candidates);
    } else {
      pending.reject(new Error(message.message));
    }
  }
}
