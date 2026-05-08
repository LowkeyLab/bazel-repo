import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { BrowserPrintService } from './browser-print.service';

describe('BrowserPrintService', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('calls window.print once', () => {
    const printSpy = vi.spyOn(window, 'print').mockImplementation(() => {});
    const service = TestBed.inject(BrowserPrintService);

    service.print();

    expect(printSpy).toHaveBeenCalledTimes(1);
  });

  it('propagates print errors', () => {
    const printError = new Error('print failed');
    vi.spyOn(window, 'print').mockImplementation(() => {
      throw printError;
    });
    const service = TestBed.inject(BrowserPrintService);

    expect(() => service.print()).toThrow(printError);
  });
});
