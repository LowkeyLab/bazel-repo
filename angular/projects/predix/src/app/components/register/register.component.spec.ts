import { ComponentFixture, TestBed } from '@angular/core/testing';
import {
  ActivatedRoute,
  Router,
  RouterLink,
  convertToParamMap,
} from '@angular/router';
import { RouterTestingModule } from '@angular/router/testing';
import { of } from 'rxjs';

import { RegisterComponent } from './register.component';
import { AuthService, type LoginResponse } from '../../services/auth.service';

describe('RegisterComponent', () => {
  let fixture: ComponentFixture<RegisterComponent>;
  let component: RegisterComponent;
  let auth: jasmine.SpyObj<AuthService>;
  let router: jasmine.SpyObj<Router>;

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

    router = jasmine.createSpyObj<Router>('Router', ['navigateByUrl']);

    await TestBed.configureTestingModule({
      imports: [
        RegisterComponent,
        RouterTestingModule.withRoutes([]),
        RouterLink,
      ],
      providers: [
        { provide: AuthService, useValue: auth },
        { provide: Router, useValue: router },
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { queryParamMap: convertToParamMap({}) } },
        },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(RegisterComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('submits registration and redirects', () => {
    component.username = 'alice';
    component.password = 'secret123';
    component.confirm = 'secret123';

    component.onSubmit(new Event('submit'));

    expect(auth.register).toHaveBeenCalledWith('alice', 'secret123');
    expect(router.navigateByUrl).toHaveBeenCalledWith('/circles');
  });
});
