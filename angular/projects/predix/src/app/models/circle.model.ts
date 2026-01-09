import { UserId } from './user.model';

export interface Circle {
  id: number;
  name: string;
  created_at: string;
  members: CircleMember[];
}

export interface CircleMember {
  user_id: UserId;
  username: string;
  clout: number;
}

export interface CreateCircleRequest {
  name: string;
  // TODO: Add UserId here if needed for request? Usually backend infers from auth token.
}

export interface AddMemberRequest {
  user_id: UserId;
}
