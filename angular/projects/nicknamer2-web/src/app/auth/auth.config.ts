import Sdk from 'casdoor-js-sdk';
import { environment } from '../../environments/environment';

if (
  !environment.casdoorClientId ||
  environment.casdoorClientId === '<your-client-id>'
) {
  console.error(
    'Casdoor client ID is not configured. ' +
      'Set casdoorClientId in environments/environment.ts (or environment.development.ts).',
  );
}

export const casdoorSdk = new Sdk({
  serverUrl: environment.casdoorServerUrl,
  clientId: environment.casdoorClientId,
  appName: environment.casdoorAppName,
  organizationName: environment.casdoorOrgName,
  redirectPath: '/callback',
});
