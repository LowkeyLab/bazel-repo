import {
  ComponentFixture,
  TestBed,
  fakeAsync,
  tick,
  discardPeriodicTasks,
} from '@angular/core/testing';
import { Router } from '@angular/router';
import { Location } from '@angular/common';
import { LandingComponent } from './landing.component';
import { GameService } from '../services/game.service';
import { provideRouter } from '@angular/router';
import { routes } from '../app.routes';
import { of } from 'rxjs';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';

// Mock GameService
class MockGameService {
  getSummary = jasmine
    .createSpy()
    .and.returnValue(of({ inProgressGames: 0, waitingForPlayerGames: 0, completedGames: 0 }));
}

describe('LandingComponent', () => {
  let component: LandingComponent;
  let fixture: ComponentFixture<LandingComponent>;
  let router: Router;
  let location: Location;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [LandingComponent],
      providers: [
        provideRouter(routes),
        provideHttpClient(),
        provideHttpClientTesting(),
        { provide: GameService, useClass: MockGameService },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(LandingComponent);
    component = fixture.componentInstance;
    router = TestBed.inject(Router);
    location = TestBed.inject(Location);
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should display the "Browse Games" button', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const button = compiled.querySelector('button.btn-primary');
    expect(button).toBeTruthy();
    expect(button?.textContent?.trim()).toBe('Browse Games');
  });

  it('should have routerLink="/games" on the Browse Games button', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const button = compiled.querySelector('button[routerLink="/games"]');
    expect(button).toBeTruthy();
  });

  it('should navigate to /games when Browse Games button is clicked', fakeAsync(() => {
    const compiled = fixture.nativeElement as HTMLElement;
    const button = compiled.querySelector('button.btn-primary') as HTMLButtonElement;
    expect(button).toBeTruthy();

    // Click the button
    button.click();
    tick(0);

    // Verify navigation occurred
    expect(location.path()).toBe('/games');
    discardPeriodicTasks();
  }));

  it('should display the component title', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const title = compiled.querySelector('h1');
    expect(title?.textContent).toBe('Mindreadr');
  });

  it('should display the component description', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const description = compiled.querySelector('p.opacity-70');
    expect(description?.textContent).toBe('A cooperative word-guessing game for two players.');
  });

  it('should display the "How to Play" section', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const howToPlayTitle = compiled.querySelector('.card-title');
    expect(howToPlayTitle?.textContent).toBe('How to Play');
  });

  it('should include the games-count component', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const gamesCountElement = compiled.querySelector('mindreadr-games-count');
    expect(gamesCountElement).toBeTruthy();
  });
});
