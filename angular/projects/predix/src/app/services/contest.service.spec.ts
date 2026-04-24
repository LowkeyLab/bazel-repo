import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import {
  HttpTestingController,
  provideHttpClientTesting,
} from '@angular/common/http/testing';
import { firstValueFrom } from 'rxjs';
import { take } from 'rxjs/operators';
import { ContestService } from './contest.service';
import { environment } from '../../environments/environment';
import type { Contest } from '../models/contest.model';
import { provideHttpClient } from '@angular/common/http';
import { AuthService } from './auth.service';

describe('ContestService', () => {
  let service: ContestService;
  let httpMock: HttpTestingController;
  const apiUrl = `${environment.apiUrl}/protected`;
  const mockAuthService = {
    token: vi.fn().mockReturnValue('test-token'),
  };
  let mockEventSource: MockEventSource;

  class MockEventSource {
    onopen: (() => void) | null = null;
    onerror: ((error: unknown) => void) | null = null;
    addEventListener = vi.fn();
    close = vi.fn();
    readyState = 1;

    constructor(public url: string) {
      mockEventSource = this;
    }
  }

  beforeEach(() => {
    (window as any).EventSource = MockEventSource as typeof EventSource;

    TestBed.configureTestingModule({
      providers: [
        ContestService,
        provideHttpClient(),
        provideHttpClientTesting(),
        { provide: AuthService, useValue: mockAuthService },
      ],
    });

    service = TestBed.inject(ContestService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  describe('getContest', () => {
    it('should fetch a contest by circleId and contestId', async () => {
      const circleId = 1;
      const contestId = 42;
      const mockContest: Contest = {
        id: contestId,
        circle_id: circleId,
        creator_id: 1,
        question: 'Who will win?',
        options: [
          { id: 1, text: 'Option A' },
          { id: 2, text: 'Option B' },
        ],
        predictions: [],
        status: 'OPEN',
        min_stake: 10,
        total_pot: 0,
        house_rake: 0,
        created_at: '2024-01-01T00:00:00Z',
        locked_at: '2024-01-02T00:00:00Z',
        duration: '1d',
      };

      const contestPromise = firstValueFrom(
        service.getContest(circleId, contestId),
      );

      const req = httpMock.expectOne(
        `${apiUrl}/circles/${circleId}/contests/${contestId}`,
      );
      expect(req.request.method).toBe('GET');
      req.flush(mockContest);
      await expect(contestPromise).resolves.toEqual(mockContest);
    });
  });

  describe('streamContestDetails', () => {
    it('should stream contest updates with totals', async () => {
      const circleId = 1;
      const contestId = 42;
      const mockContest: Contest = {
        id: contestId,
        circle_id: circleId,
        creator_id: 1,
        question: 'Who will win?',
        options: [
          { id: 1, text: 'Option A' },
          { id: 2, text: 'Option B' },
        ],
        predictions: [
          {
            option_id: 1,
            clout: 50,
            user_id: 1,
            timestamp: '2024-01-01T00:00:00Z',
          },
          {
            option_id: 1,
            clout: 30,
            user_id: 2,
            timestamp: '2024-01-01T00:00:00Z',
          },
        ],
        status: 'OPEN',
        min_stake: 10,
        total_pot: 80,
        house_rake: 0,
        created_at: '2024-01-01T00:00:00Z',
        locked_at: '2024-01-02T00:00:00Z',
        duration: '1d',
      };

      const resultPromise = firstValueFrom(
        service.streamContestDetails(circleId, contestId).pipe(take(1)),
      );

      mockEventSource.onopen?.();
      const req = httpMock.expectOne(
        `${apiUrl}/circles/${circleId}/contests/${contestId}`,
      );
      req.flush(mockContest);

      const result = await resultPromise;
      expect(result.id).toBe(contestId);
      expect(result.totals.byOption.get(1)?.clout).toBe(80);
      expect(result.totals.byOption.get(1)?.count).toBe(2);
    });
  });
});
