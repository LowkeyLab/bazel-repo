import { Injectable } from '@angular/core';

export interface HanziGuess {
  readonly text: string;
  readonly score: number;
}

export interface PinyinchchWasmModule {
  readonly default: () => Promise<unknown>;
  readonly guess_hanzi: (pinyin: string, limit: number) => string;
}

@Injectable({ providedIn: 'root' })
export class PinyinchchService {
  private wasmModulePromise: Promise<PinyinchchWasmModule> | null = null;

  async guess(input: string, limit = 5): Promise<readonly HanziGuess[]> {
    const trimmedInput = input.trim();

    if (!trimmedInput) {
      return [];
    }

    const wasmModule = await this.loadWasmModule();
    const rawResult = wasmModule.guess_hanzi(trimmedInput, limit);

    return this.parseResult(rawResult);
  }

  private async loadWasmModule(): Promise<PinyinchchWasmModule> {
    this.wasmModulePromise ??= import('./wasm/pinyinchch_wasm.js').then(
      async (wasmModule: PinyinchchWasmModule) => {
        await wasmModule.default();
        return wasmModule;
      },
    );

    return this.wasmModulePromise;
  }

  private parseResult(rawResult: string): readonly HanziGuess[] {
    const parsedResult: unknown = JSON.parse(rawResult);

    if (!Array.isArray(parsedResult)) {
      throw new Error('pinyinchch returned an invalid result');
    }

    return parsedResult.map((item) => {
      if (!this.isHanziGuess(item)) {
        throw new Error('pinyinchch returned an invalid candidate');
      }

      return item;
    });
  }

  private isHanziGuess(value: unknown): value is HanziGuess {
    return (
      typeof value === 'object' &&
      value !== null &&
      'text' in value &&
      'score' in value &&
      typeof value.text === 'string' &&
      typeof value.score === 'number'
    );
  }
}
