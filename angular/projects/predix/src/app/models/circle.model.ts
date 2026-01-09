export interface Circle {
  id: number;
  name: string;
  created_at: string;
  members: CircleMember[];
}

export interface CircleMember {
  user_id: number | string;
  username: string;
  clout: number;
}

export interface CreateCircleRequest {
  name: string;
}

export interface AddMemberRequest {
  user_id: number | string;
}
