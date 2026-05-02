import { ComponentFixture, TestBed } from '@angular/core/testing';

import { RubyPreviewComponent } from './ruby-preview.component';

describe('RubyPreviewComponent', () => {
  let fixture: ComponentFixture<RubyPreviewComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [RubyPreviewComponent],
    }).compileComponents();
    fixture = TestBed.createComponent(RubyPreviewComponent);
  });

  it('renders one ruby unit per phrase token', () => {
    fixture.componentRef.setInput('tokens', [
      {
        id: 'token-1',
        sourcePinyin: 'beijing',
        hanzi: '北京',
        displayPinyin: 'Běijīng',
      },
    ]);
    fixture.detectChanges();

    const ruby = fixture.nativeElement.querySelector(
      '[data-testid="ruby-token"]',
    ) as HTMLElement;
    expect(ruby.textContent).toContain('北京');
    expect(ruby.textContent).toContain('Běijīng');
  });
});
