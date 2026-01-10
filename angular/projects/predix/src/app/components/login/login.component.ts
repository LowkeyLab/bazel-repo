import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
} from '@angular/core';
import { Router, ActivatedRoute } from '@angular/router';

import { AuthService } from '../../services/auth.service';
import { LocalStorageService } from '../../services/local-storage.service';

@Component({
  selector: 'login',
  imports: [],
  templateUrl: './login.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoginComponent implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);
  private readonly localStorageService = inject(LocalStorageService);

  ngOnInit(): void {
    if (this.auth.isAuthenticated()) {
      this.router.navigate(['/circles']);
    }
  }

  protected onLogin(): void {
    const redirectTo = this.route.snapshot.queryParamMap.get('redirectTo');
    if (redirectTo) {
      this.localStorageService.setPostLoginRedirect(redirectTo);
    }
    this.auth.login();
  }
}
