export type ContestStatus = 'OPEN' | 'LOCKED' | 'CLOSED' | 'RESOLVED';

export interface Contest {
  id: number;
  circle_id: number;
  creator_id: number;
  question: string;
  options: ContestOption[];
  predictions: Prediction[];
  status: ContestStatus;
  min_stake: number;
  total_pot: number;
  house_rake: number;
  result_option_id?: number;
  created_at: string;
  closes_at: string;
  duration: string;
}

// Computed field: 90% of total pot after 10% house rake
export function getDistributableClout(contest: Contest): number {
  return contest.total_pot - contest.house_rake;
}

// Computed field: 10% of total pot (house fee)
export function getHouseRakeClout(contest: Contest): number {
  return contest.house_rake;
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
  circle_id: number;
  question: string;
  options: string[];
  min_stake: number;
  expiration_duration: string;
}

export interface MakePredictionRequest {
  option_id: number;
  clout: number;
}

export interface ResolveContestRequest {
  winning_option_id: number;
}

export interface PayoutRecord {
  user_id: number;
  username: string;
  stake: number;
  share: number;
  total: number;
}

export interface PayoutBreakdown {
  winners: PayoutRecord[];
  losers: PayoutRecord[];
  total_pot: number;
  house_rake: number;
  distributable_pot: number;
  total_distributed: number;
}
