import { TestBed } from '@angular/core/testing';
import {
  HttpTestingController,
  provideHttpClientTesting,
} from '@angular/common/http/testing';
import { TestScheduler } from 'rxjs/testing';
import { ContestService } from './contest.service';
import { environment } from '../../environments/environment';
import type { Contest } from '../models/contest.model';
import { provideHttpClient } from '@angular/common/http';

describe('ContestService', () => {
  let service: ContestService;
  let httpMock: HttpTestingController;
  const apiUrl = `${environment.apiUrl}/protected`;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        ContestService,
        provideHttpClient(),
        provideHttpClientTesting(),
      ],
    });

    service = TestBed.inject(ContestService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  describe('getContest', () => {
    it('should fetch a contest by circleId and contestId', (done) => {
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
        closes_at: '2024-01-02T00:00:00Z',
        duration: '1d',
      };

      service.getContest(circleId, contestId).subscribe({
        next: (contest) => {
          expect(contest).toEqual(mockContest);
          done();
        },
      });

      const req = httpMock.expectOne(
        `${apiUrl}/circles/${circleId}/contests/${contestId}`,
      );
      expect(req.request.method).toBe('GET');
      req.flush(mockContest);
    });
  });

  describe('pollContestDetails', () => {
    let scheduler: TestScheduler;

    beforeEach(() => {
      scheduler = new TestScheduler((actual, expected) => {
        expect(actual).toEqual(expected);
      });
    });

    it('should poll for contest details and compute totals', (done) => {
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
            user_id: 1,
            option_id: 1,
            clout: 100,
            timestamp: '2024-01-01T00:00:00Z',
          },
          {
            user_id: 2,
            option_id: 1,
            clout: 50,
            timestamp: '2024-01-01T00:00:01Z',
          },
          {
            user_id: 3,
            option_id: 2,
            clout: 200,
            timestamp: '2024-01-01T00:00:02Z',
          },
        ],
        status: 'OPEN',
        min_stake: 10,
        total_pot: 350,
        house_rake: 35,
        created_at: '2024-01-01T00:00:00Z',
        closes_at: '2024-01-02T00:00:00Z',
        duration: '1d',
      };

      let emissionCount = 0;
      const subscription = service
        .pollContestDetails(circleId, contestId, 100)
        .subscribe({
          next: (contestWithTotals) => {
            emissionCount++;
            expect(contestWithTotals.id).toBe(contestId);
            expect(contestWithTotals.totals.byOption.get(1)).toEqual({
              clout: 150,
              count: 2,
            });
            expect(contestWithTotals.totals.byOption.get(2)).toEqual({
              clout: 200,
              count: 1,
            });

            if (emissionCount === 2) {
              subscription.unsubscribe();
              done();
            }
          },
        });

      // First poll
      const req1 = httpMock.expectOne(
        `${apiUrl}/circles/${circleId}/contests/${contestId}`,
      );
      req1.flush(mockContest);

      // Second poll
      setTimeout(() => {
        const req2 = httpMock.expectOne(
          `${apiUrl}/circles/${circleId}/contests/${contestId}`,
        );
        req2.flush(mockContest);
      }, 100);
    });

    it('should stop polling when contest status becomes CLOSED', (done) => {
      const circleId = 1;
      const contestId = 42;
      const mockContestOpen: Contest = {
        id: contestId,
        circle_id: circleId,
        creator_id: 1,
        question: 'Who will win?',
        options: [{ id: 1, text: 'Option A' }],
        predictions: [],
        status: 'OPEN',
        min_stake: 10,
        total_pot: 0,
        house_rake: 0,
        created_at: '2024-01-01T00:00:00Z',
        closes_at: '2024-01-02T00:00:00Z',
        duration: '1d',
      };

      const mockContestClosed: Contest = {
        ...mockContestOpen,
        status: 'CLOSED',
      };

      let emissionCount = 0;
      service.pollContestDetails(circleId, contestId, 100).subscribe({
        next: (contestWithTotals) => {
          emissionCount++;
          if (emissionCount === 1) {
            expect(contestWithTotals.status).toBe('OPEN');
          } else if (emissionCount === 2) {
            expect(contestWithTotals.status).toBe('CLOSED');
          }
        },
        complete: () => {
          expect(emissionCount).toBe(2);
          done();
        },
      });

      // First poll: OPEN
      const req1 = httpMock.expectOne(
        `${apiUrl}/circles/${circleId}/contests/${contestId}`,
      );
      req1.flush(mockContestOpen);

      // Second poll: CLOSED (should complete after this)
      setTimeout(() => {
        const req2 = httpMock.expectOne(
          `${apiUrl}/circles/${circleId}/contests/${contestId}`,
        );
        req2.flush(mockContestClosed);
      }, 100);
    });

    it('should stop polling when contest status becomes RESOLVED', (done) => {
      const circleId = 1;
      const contestId = 42;
      const mockContestLocked: Contest = {
        id: contestId,
        circle_id: circleId,
        creator_id: 1,
        question: 'Who will win?',
        options: [{ id: 1, text: 'Option A' }],
        predictions: [],
        status: 'LOCKED',
        min_stake: 10,
        total_pot: 0,
        house_rake: 0,
        created_at: '2024-01-01T00:00:00Z',
        closes_at: '2024-01-02T00:00:00Z',
        duration: '1d',
      };

      const mockContestResolved: Contest = {
        ...mockContestLocked,
        status: 'RESOLVED',
        result_option_id: 1,
      };

      let emissionCount = 0;
      service.pollContestDetails(circleId, contestId, 100).subscribe({
        next: (contestWithTotals) => {
          emissionCount++;
          if (emissionCount === 1) {
            expect(contestWithTotals.status).toBe('LOCKED');
          } else if (emissionCount === 2) {
            expect(contestWithTotals.status).toBe('RESOLVED');
          }
        },
        complete: () => {
          expect(emissionCount).toBe(2);
          done();
        },
      });

      // First poll: LOCKED
      const req1 = httpMock.expectOne(
        `${apiUrl}/circles/${circleId}/contests/${contestId}`,
      );
      req1.flush(mockContestLocked);

      // Second poll: RESOLVED (should complete after this)
      setTimeout(() => {
        const req2 = httpMock.expectOne(
          `${apiUrl}/circles/${circleId}/contests/${contestId}`,
        );
        req2.flush(mockContestResolved);
      }, 100);
    });
  });
});
