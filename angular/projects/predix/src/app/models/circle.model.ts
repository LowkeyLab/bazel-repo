export interface Circle {
  id: number;
  name: string;
  created_at: string;
  members: CircleMember[];
}

export interface CircleMember {
  user_id: number;
  clout: number;
}

export interface CreateCircleRequest {
  name: string;
  creator_id: number;
}

export interface AddMemberRequest {
  user_id: number;
}
