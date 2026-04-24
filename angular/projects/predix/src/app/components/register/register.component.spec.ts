import { ComponentFixture, TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { provideRouter, Router } from '@angular/router';
import { provideLocationMocks } from '@angular/common/testing';
import { of } from 'rxjs';

import { RegisterComponent } from './register.component';
import { AuthService, type LoginResponse } from '../../services/auth.service';
import { createMockObject } from '../../../testing/create-mock-object';

describe('RegisterComponent', () => {
  const authMethods = ['isAuthenticated', 'register'] as const;
  let fixture: ComponentFixture<RegisterComponent>;
  let component: RegisterComponent;
  let auth = createMockObject(authMethods);
  let router: Router;

  beforeEach(async () => {
    auth = createMockObject(authMethods);
    const loginResponse: LoginResponse = {
      token: 'token',
      user: { id: 1, username: 'alice', role: 'member' },
    };
    auth.isAuthenticated.mockReturnValue(false);
    auth.register.mockReturnValue(of(loginResponse));

    await TestBed.configureTestingModule({
      imports: [RegisterComponent],
      providers: [
        provideRouter([]),
        provideLocationMocks(),
        { provide: AuthService, useValue: auth as unknown as AuthService },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(RegisterComponent);
    component = fixture.componentInstance;
    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigateByUrl');
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('submits registration and redirects', () => {
    // Type assertion to access protected properties in tests
    const comp = component as any;
    comp.username = 'alice';
    comp.password = 'secret123';
    comp.confirm = 'secret123';

    comp.onSubmit(new Event('submit'));

    expect(auth.register).toHaveBeenCalledWith('alice', 'secret123');
    expect(router.navigateByUrl).toHaveBeenCalledWith('/circles');
  });
});
