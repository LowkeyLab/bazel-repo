import { inject } from '@angular/core';
import { InMemoryCache } from '@apollo/client';
import { relayStylePagination } from '@apollo/client/utilities';
import { provideApollo } from 'apollo-angular';
import { HttpLink } from 'apollo-angular/http';
import { environment } from '../environments/environment';

const normalizedBackendUrl =
  environment.backendUrl ||
  (typeof window !== 'undefined' ? window.location.origin : '');

export const BACKEND_URL = normalizedBackendUrl;
const GRAPHQL_URI = `${BACKEND_URL}/graphql`;

export function provideGraphql() {
  return provideApollo(() => {
    const httpLink = inject(HttpLink);

    return {
      link: httpLink.create({ uri: GRAPHQL_URI }),
      cache: new InMemoryCache({
        typePolicies: {
          Query: {
            fields: {
              servers: relayStylePagination(),
            },
          },
          Server: {
            fields: {
              names: relayStylePagination(),
            },
          },
        },
      }),
    };
  });
}
