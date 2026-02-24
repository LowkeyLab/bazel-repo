import { inject } from '@angular/core';
import { InMemoryCache } from '@apollo/client';
import { provideApollo } from 'apollo-angular';
import { HttpLink } from 'apollo-angular/http';

const GRAPHQL_URI = 'http://localhost:8080/graphql';

export function provideGraphql() {
  return provideApollo(() => {
    const httpLink = inject(HttpLink);

    return {
      link: httpLink.create({ uri: GRAPHQL_URI }),
      cache: new InMemoryCache({
        typePolicies: {
          Query: {
            fields: {
              servers: {
                keyArgs: false,
                merge(existing, incoming) {
                  const edges = existing
                    ? [...existing.edges, ...incoming.edges]
                    : incoming.edges;
                  return { ...incoming, edges };
                },
              },
            },
          },
          Server: {
            fields: {
              names: {
                keyArgs: false,
                merge(existing, incoming) {
                  const edges = existing
                    ? [...existing.edges, ...incoming.edges]
                    : incoming.edges;
                  return { ...incoming, edges };
                },
              },
            },
          },
        },
      }),
    };
  });
}
