export type UserId = number | string;

export interface User {
  id: UserId;
  name: string;
  email: string;
}

export interface CreateUserRequest {
  name: string;
  email: string;
}
