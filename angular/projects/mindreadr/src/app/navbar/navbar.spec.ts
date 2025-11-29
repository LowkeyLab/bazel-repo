import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { Location } from '@angular/common';
import { Navbar } from './navbar';
import { Component } from '@angular/core';

@Component({
  standalone: true,
  template: '<h1>Home Page</h1>',
})
class HomeComponent {}

@Component({
  standalone: true,
  template: '<h1>Games Page</h1>',
})
class GamesComponent {}

describe('Navbar', () => {
  let component: Navbar;
  let fixture: ComponentFixture<Navbar>;
  let router: Router;
  let location: Location;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [Navbar],
      providers: [
        provideRouter([
          { path: '', component: HomeComponent },
          { path: 'games', component: GamesComponent },
        ]),
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(Navbar);
    component = fixture.componentInstance;
    router = TestBed.inject(Router);
    location = TestBed.inject(Location);
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should render the Mindreadr brand link', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const brandLink = compiled.querySelector('a.btn-ghost');
    expect(brandLink?.textContent).toContain('Mindreadr');
  });

  it('should render Home and Games navigation links', () => {
    const compiled = fixture.nativeElement as HTMLElement;
    const navLinks = compiled.querySelectorAll('nav a');
    expect(navLinks.length).toBeGreaterThanOrEqual(2);
    expect(navLinks[0].textContent).toContain('Home');
    expect(navLinks[1].textContent).toContain('Games');
  });

  it('should navigate to home page when Home link is clicked', async () => {
    // Start from games page
    await router.navigate(['/games']);
    fixture.detectChanges();
    expect(router.url).toBe('/games');

    const compiled = fixture.nativeElement as HTMLElement;
    const homeLink = compiled.querySelector('nav a[routerLink="/"]') as HTMLAnchorElement;

    homeLink.click();
    await fixture.whenStable();

    expect(router.url).toBe('/');
  });

  it('should navigate to games page when Games link is clicked', async () => {
    // Start from home
    await router.navigate(['/']);
    fixture.detectChanges();
    expect(router.url).toBe('/');

    const compiled = fixture.nativeElement as HTMLElement;
    const gamesLink = compiled.querySelector('nav a[routerLink="/games"]') as HTMLAnchorElement;

    gamesLink.click();
    await fixture.whenStable();

    expect(router.url).toBe('/games');
  });

  it('should navigate to home when brand link is clicked', async () => {
    // First navigate to games
    await router.navigate(['/games']);
    fixture.detectChanges();
    expect(router.url).toBe('/games');

    // Click brand link
    const compiled = fixture.nativeElement as HTMLElement;
    const brandLink = compiled.querySelector('a.btn-ghost') as HTMLAnchorElement;
    brandLink.click();
    await fixture.whenStable();

    expect(router.url).toBe('/');
  });
});
