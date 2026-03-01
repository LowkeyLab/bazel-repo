import { gql } from 'apollo-angular';
import { Injectable } from '@angular/core';
import * as Apollo from 'apollo-angular';
export type Maybe<T> = T | null;
export type InputMaybe<T> = Maybe<T>;
export type Exact<T extends { [key: string]: unknown }> = {
  [K in keyof T]: T[K];
};
export type MakeOptional<T, K extends keyof T> = Omit<T, K> & {
  [SubKey in K]?: Maybe<T[SubKey]>;
};
export type MakeMaybe<T, K extends keyof T> = Omit<T, K> & {
  [SubKey in K]: Maybe<T[SubKey]>;
};
export type MakeEmpty<
  T extends { [key: string]: unknown },
  K extends keyof T,
> = { [_ in K]?: never };
export type Incremental<T> =
  | T
  | {
      [P in keyof T]?: P extends ' $fragmentName' | '__typename' ? T[P] : never;
    };
/** All built-in and custom scalars, mapped to their actual values */
export type Scalars = {
  ID: { input: string; output: string };
  String: { input: string; output: string };
  Boolean: { input: boolean; output: boolean };
  Int: { input: number; output: number };
  Float: { input: number; output: number };
  /**
   * Combined date and time (with time zone) in [RFC 3339][0] format.
   *
   * Represents a description of an exact instant on the time-line (such as the
   * instant that a user account was created).
   *
   * [`DateTime` scalar][1] compliant.
   *
   * See also [`chrono::DateTime`][2] for details.
   *
   * [0]: https://datatracker.ietf.org/doc/html/rfc3339#section-5
   * [1]: https://graphql-scalars.dev/docs/scalars/date-time
   * [2]: https://docs.rs/chrono/latest/chrono/struct.DateTime.html
   */
  DateTime: { input: any; output: any };
};

/** A name associated with a user in a Discord server */
export type Name = Node & {
  __typename?: 'Name';
  createdAt: Scalars['DateTime']['output'];
  /** The globally unique Relay ID for this Name */
  id: Scalars['ID']['output'];
  name: Scalars['String']['output'];
  updatedAt: Scalars['DateTime']['output'];
};

export type NameConnection = {
  __typename?: 'NameConnection';
  /** The list of edges */
  edges: Array<NameEdge>;
  /** Information about pagination */
  pageInfo: PageInfo;
  /** Total number of names */
  totalCount: Scalars['Int']['output'];
};

export type NameEdge = {
  __typename?: 'NameEdge';
  /** The cursor for this edge, used for pagination */
  cursor: Scalars['String']['output'];
  /** The Name node */
  node: Name;
};

/** The Relay Node interface - all types with global IDs implement this */
export type Node = {
  /** The globally unique ID for this object */
  id: Scalars['ID']['output'];
};

export type PageInfo = {
  __typename?: 'PageInfo';
  /** The cursor of the last item in the list */
  endCursor?: Maybe<Scalars['String']['output']>;
  /** Whether there are more items when paginating forwards */
  hasNextPage: Scalars['Boolean']['output'];
  /** Whether there are more items when paginating backwards */
  hasPreviousPage: Scalars['Boolean']['output'];
  /** The cursor of the first item in the list */
  startCursor?: Maybe<Scalars['String']['output']>;
};

export type QueryRoot = {
  __typename?: 'QueryRoot';
  /** Fetch any object by its global Relay ID. */
  node?: Maybe<Node>;
  /** Fetch a Discord server by its ID */
  server: Server;
  /** Paginated list of all servers */
  servers: ServerConnection;
};

export type QueryRootNodeArgs = {
  id: Scalars['ID']['input'];
};

export type QueryRootServerArgs = {
  id: Scalars['ID']['input'];
};

export type QueryRootServersArgs = {
  after?: InputMaybe<Scalars['String']['input']>;
  first?: InputMaybe<Scalars['Int']['input']>;
};

/** A Discord server */
export type Server = Node & {
  __typename?: 'Server';
  /** The server ID (global Relay ID) */
  id: Scalars['ID']['output'];
  /** Paginated list of names in this server */
  names: NameConnection;
  /** The Discord server ID */
  serverId: Scalars['String']['output'];
};

/** A Discord server */
export type ServerNamesArgs = {
  after?: InputMaybe<Scalars['String']['input']>;
  first?: InputMaybe<Scalars['Int']['input']>;
};

export type ServerConnection = {
  __typename?: 'ServerConnection';
  /** The list of edges */
  edges: Array<ServerEdge>;
  /** Information about pagination */
  pageInfo: PageInfo;
  /** Total number of servers */
  totalCount: Scalars['Int']['output'];
};

export type ServerEdge = {
  __typename?: 'ServerEdge';
  /** The cursor for this edge, used for pagination */
  cursor: Scalars['String']['output'];
  /** The Server node */
  node: Server;
};

export type CreateNameInput = {
  clientMutationId?: InputMaybe<Scalars['String']['input']>;
  discordId: Scalars['String']['input'];
  discordServerId: Scalars['String']['input'];
  name: Scalars['String']['input'];
};

export type CreateNamePayload = {
  __typename?: 'CreateNamePayload';
  clientMutationId?: Maybe<Scalars['String']['output']>;
  name: {
    __typename?: 'Name';
    id: string;
    name: string;
    createdAt: any;
    updatedAt: any;
  };
};

export type CreateNameMutationVariables = Exact<{
  input: CreateNameInput;
}>;

export type CreateNameMutation = {
  __typename?: 'MutationRoot';
  createName: CreateNamePayload;
};

export type GetDashboardQueryVariables = Exact<{
  first: Scalars['Int']['input'];
}>;

export type GetDashboardQuery = {
  __typename?: 'QueryRoot';
  servers: {
    __typename?: 'ServerConnection';
    totalCount: number;
    edges: Array<{
      __typename?: 'ServerEdge';
      node: { __typename?: 'Server'; id: string; serverId: string };
    }>;
  };
};

export type GetServerNamesQueryVariables = Exact<{
  id: Scalars['ID']['input'];
  first: Scalars['Int']['input'];
  after?: InputMaybe<Scalars['String']['input']>;
}>;

export type GetServerNamesQuery = {
  __typename?: 'QueryRoot';
  server: {
    __typename?: 'Server';
    id: string;
    serverId: string;
    names: {
      __typename?: 'NameConnection';
      edges: Array<{
        __typename?: 'NameEdge';
        cursor: string;
        node: {
          __typename?: 'Name';
          id: string;
          name: string;
          createdAt: any;
          updatedAt: any;
        };
      }>;
      pageInfo: {
        __typename?: 'PageInfo';
        hasNextPage: boolean;
        endCursor?: string | null;
      };
    };
  };
};

export type GetServersQueryVariables = Exact<{
  first: Scalars['Int']['input'];
  after?: InputMaybe<Scalars['String']['input']>;
}>;

export type GetServersQuery = {
  __typename?: 'QueryRoot';
  servers: {
    __typename?: 'ServerConnection';
    edges: Array<{
      __typename?: 'ServerEdge';
      cursor: string;
      node: { __typename?: 'Server'; id: string; serverId: string };
    }>;
    pageInfo: {
      __typename?: 'PageInfo';
      hasNextPage: boolean;
      endCursor?: string | null;
    };
  };
};

export const GetDashboardDocument = gql`
  query GetDashboard($first: Int!) {
    servers(first: $first) {
      totalCount
      edges {
        node {
          id
          serverId
        }
      }
    }
  }
`;

@Injectable({
  providedIn: 'root',
})
export class GetDashboardGQL extends Apollo.Query<
  GetDashboardQuery,
  GetDashboardQueryVariables
> {
  document = GetDashboardDocument;

  constructor(apollo: Apollo.Apollo) {
    super(apollo);
  }
}
export const GetServerNamesDocument = gql`
  query GetServerNames($id: ID!, $first: Int!, $after: String) {
    server(id: $id) {
      id
      serverId
      names(first: $first, after: $after) {
        edges {
          cursor
          node {
            id
            name
            createdAt
            updatedAt
          }
        }
        pageInfo {
          hasNextPage
          endCursor
        }
      }
    }
  }
`;

@Injectable({
  providedIn: 'root',
})
export class GetServerNamesGQL extends Apollo.Query<
  GetServerNamesQuery,
  GetServerNamesQueryVariables
> {
  document = GetServerNamesDocument;

  constructor(apollo: Apollo.Apollo) {
    super(apollo);
  }
}
export const GetServersDocument = gql`
  query GetServers($first: Int!, $after: String) {
    servers(first: $first, after: $after) {
      edges {
        cursor
        node {
          id
          serverId
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

@Injectable({
  providedIn: 'root',
})
export class GetServersGQL extends Apollo.Query<
  GetServersQuery,
  GetServersQueryVariables
> {
  document = GetServersDocument;

  constructor(apollo: Apollo.Apollo) {
    super(apollo);
  }
}
export const CreateNameDocument = gql`
  mutation CreateName($input: CreateNameInput!) {
    createName(input: $input) {
      clientMutationId
      name {
        id
        name
        createdAt
        updatedAt
      }
    }
  }
`;

@Injectable({
  providedIn: 'root',
})
export class CreateNameGQL extends Apollo.Mutation<
  CreateNameMutation,
  CreateNameMutationVariables
> {
  document = CreateNameDocument;

  constructor(apollo: Apollo.Apollo) {
    super(apollo);
  }
}
