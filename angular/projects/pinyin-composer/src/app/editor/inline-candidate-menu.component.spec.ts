import { type ComponentFixture, TestBed } from '@angular/core/testing';

import { InlineCandidateMenuComponent } from './inline-candidate-menu.component';

describe('InlineCandidateMenuComponent', () => {
  let fixture: ComponentFixture<InlineCandidateMenuComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [InlineCandidateMenuComponent],
    }).compileComponents();
    fixture = TestBed.createComponent(InlineCandidateMenuComponent);
  });

  it('emits the clicked candidate', () => {
    const selected: string[] = [];
    fixture.componentRef.setInput('candidates', [
      {
        id: 'candidate-0',
        sourcePinyin: 'beijing',
        sourcePinyinSyllables: ['bei', 'jing'],
        hanzi: '北京',
        displayPinyin: 'Běijīng',
        displayPinyinSyllables: ['Běi', 'jīng'],
        score: 1,
      },
    ]);
    fixture.componentInstance.candidateSelected.subscribe((candidate) =>
      selected.push(candidate.id),
    );
    fixture.detectChanges();

    (
      fixture.nativeElement.querySelector(
        '[data-testid="candidate-option"]',
      ) as HTMLButtonElement
    ).click();

    expect(selected).toEqual(['candidate-0']);
  });
});
