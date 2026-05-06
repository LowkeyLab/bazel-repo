import { TestBed } from '@angular/core/testing';

import type { Candidate } from '../editor/phrase-token';
import {
  CONVERSION_WORKER_FACTORY,
  ConversionWorkerClient,
} from './conversion-worker.client';

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
        candidates: [beijingCandidate()],
      },
    });

    await expect(promise).resolves.toEqual([beijingCandidate()]);
  });

  it('rejects failed conversion responses', async () => {
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

    const promise = client.convertPinyin('???', 5);
    worker.reply({
      requestId: worker.lastRequestId,
      type: 'conversion-error',
      message: 'conversion unavailable',
    });

    await expect(promise).rejects.toThrow('conversion unavailable');
  });

  it('ignores responses for unknown request ids', async () => {
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

    const promise = client.convertPinyin('ni hao', 5);
    worker.reply({
      requestId: 'stale-request',
      type: 'conversion-error',
      message: 'stale response',
    });
    worker.reply({
      requestId: worker.lastRequestId,
      type: 'conversion-result',
      result: {
        sourcePinyin: 'ni hao',
        candidates: [niHaoCandidate()],
      },
    });

    await expect(promise).resolves.toEqual([niHaoCandidate()]);
  });

  it('terminates the worker when disposed', () => {
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

    client.convertPinyin('beijing', 5);
    client.dispose();

    expect(worker.terminated).toBe(true);
  });
});

class FakeWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  lastRequestId = '';
  terminated = false;

  postMessage(message: { requestId: string }): void {
    this.lastRequestId = message.requestId;
  }

  terminate(): void {
    this.terminated = true;
  }

  reply(message: unknown): void {
    this.onmessage?.({ data: message } as MessageEvent);
  }
}

function beijingCandidate(): Candidate {
  return {
    id: 'candidate-0',
    sourcePinyin: 'beijing',
    sourcePinyinSyllables: ['bei', 'jing'],
    hanzi: '北京',
    displayPinyin: 'Běijīng',
    displayPinyinSyllables: ['Běi', 'jīng'],
    score: 1,
  };
}

function niHaoCandidate(): Candidate {
  return {
    id: 'candidate-0',
    sourcePinyin: 'ni hao',
    sourcePinyinSyllables: ['ni', 'hao'],
    hanzi: '你好',
    displayPinyin: 'Nǐhǎo',
    displayPinyinSyllables: ['Nǐ', 'hǎo'],
    score: 1,
  };
}
