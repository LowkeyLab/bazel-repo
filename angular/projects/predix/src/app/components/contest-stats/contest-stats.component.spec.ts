import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ContestStatsComponent, StatItem } from './contest-stats.component';
import { By } from '@angular/platform-browser';
import { ContestStatComponent } from '../contest-stat/contest-stat.component';

describe('ContestStatsComponent', () => {
  let component: ContestStatsComponent;
  let fixture: ComponentFixture<ContestStatsComponent>;

  const mockStats: StatItem[] = [
    { title: 'Total Contests', value: 10 },
    { title: 'Active Contests', value: 3 },
  ];

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [ContestStatsComponent, ContestStatComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(ContestStatsComponent);
    component = fixture.componentInstance;

    // Set required inputs
    fixture.componentRef.setInput('stats', mockStats);
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should render the correct number of contest-stat components', () => {
    const statComponents = fixture.debugElement.queryAll(
      By.directive(ContestStatComponent),
    );
    expect(statComponents.length).toBe(mockStats.length);
  });

  it('should pass the correct data to each contest-stat component', () => {
    const statComponents = fixture.debugElement.queryAll(
      By.directive(ContestStatComponent),
    );

    statComponents.forEach((statComp, index) => {
      expect(statComp.componentInstance.title()).toBe(mockStats[index].title);
      expect(statComp.componentInstance.value()).toBe(mockStats[index].value);
    });
  });
});
