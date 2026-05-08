import { Injectable } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class BrowserPrintService {
  print(): void {
    window.print();
  }
}
