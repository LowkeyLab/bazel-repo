export type ContestStatus = 'OPEN' | 'CLOSED' | 'RESOLVED';

export interface Contest {
  id: number;
  circle_ids: number[];
  creator_id: number;
  question: string;
  options: ContestOption[];
  predictions: Prediction[];
  status: ContestStatus;
  result_option_id?: number;
  created_at: string;
  expires_at: string;
}

export interface ContestOption {
  id: number;
  text: string;
}

export interface Prediction {
  user_id: number;
  option_id: number;
  clout: number;
  timestamp: string;
}

export interface CreateContestRequest {
  circle_ids: number[];
  question: string;
  options: string[];
  expires_at: string;
}

export interface MakePredictionRequest {
  option_id: number;
  clout: number;
}

export interface ResolveContestRequest {
  winning_option_id: number;
}
