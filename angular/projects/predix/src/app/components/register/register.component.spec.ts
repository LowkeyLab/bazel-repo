import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { provideLocationMocks } from '@angular/common/testing';
import { of } from 'rxjs';

import { RegisterComponent } from './register.component';
import { AuthService, type LoginResponse } from '../../services/auth.service';

describe('RegisterComponent', () => {
  let fixture: ComponentFixture<RegisterComponent>;
  let component: RegisterComponent;
  let auth: jasmine.SpyObj<AuthService>;
  let router: Router;

  beforeEach(async () => {
    auth = jasmine.createSpyObj<AuthService>('AuthService', [
      'isAuthenticated',
      'register',
    ]);
    const loginResponse: LoginResponse = {
      token: 'token',
      user: { id: 1, username: 'alice', role: 'member' },
    };
    auth.isAuthenticated.and.returnValue(false);
    auth.register.and.returnValue(of(loginResponse));

    await TestBed.configureTestingModule({
      imports: [RegisterComponent],
      providers: [
        provideRouter([]),
        provideLocationMocks(),
        { provide: AuthService, useValue: auth },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(RegisterComponent);
    component = fixture.componentInstance;
    router = TestBed.inject(Router);
    spyOn(router, 'navigateByUrl');
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
