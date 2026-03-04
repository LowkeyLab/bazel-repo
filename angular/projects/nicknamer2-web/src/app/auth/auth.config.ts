import Sdk from 'casdoor-js-sdk';

export const casdoorSdk = new Sdk({
  serverUrl: 'http://localhost:8000',
  clientId: '<your-client-id>',
  appName: 'nicknamer2',
  organizationName: 'built-in',
  redirectPath: '/callback',
});
