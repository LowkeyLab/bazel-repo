import { TestBed } from '@angular/core/testing';

import { CONVERSION_WORKER_FACTORY } from './conversion-worker.client';
import { ConversionWorkerClient } from './conversion-worker.client';

describe('ConversionWorkerClient', () => {
  it('requests candidates over the worker protocol', async () => {
    const worker = new FakeWorker();
    TestBed.configureTestingModule({
      providers: [
        {
          provide: CONVERSION_WORKER_FACTORY,
          useValue: () => worker as unknown as Worker,
        },
      ],
    });
    const client = TestBed.inject(ConversionWorkerClient);

    const promise = client.convertPinyin('beijing', 5);
    worker.reply({
      requestId: worker.lastRequestId,
      type: 'conversion-result',
      result: {
        sourcePinyin: 'beijing',
        candidates: [
          {
            id: 'candidate-0',
            sourcePinyin: 'beijing',
            hanzi: '北京',
            displayPinyin: 'Běijīng',
            score: 1,
          },
        ],
      },
    });

    await expect(promise).resolves.toEqual([
      {
        id: 'candidate-0',
        sourcePinyin: 'beijing',
        hanzi: '北京',
        displayPinyin: 'Běijīng',
        score: 1,
      },
    ]);
  });
});

class FakeWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  lastRequestId = '';

  postMessage(message: { requestId: string }): void {
    this.lastRequestId = message.requestId;
  }

  terminate(): void {}

  reply(message: unknown): void {
    this.onmessage?.({ data: message } as MessageEvent);
  }
}
