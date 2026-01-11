import { TestBed } from '@angular/core/testing';
import {
  HttpTestingController,
  provideHttpClientTesting,
} from '@angular/common/http/testing';
import { TestScheduler } from 'rxjs/testing';
import { of } from 'rxjs';
import { take } from 'rxjs/operators';
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
        locked_at: '2024-01-02T00:00:00Z',
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

    it('should poll for contest details and compute totals', () => {
      scheduler.run(({ expectObservable }) => {
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
          locked_at: '2024-01-02T00:00:00Z',
          duration: '1d',
        };

        spyOn(service, 'getContest').and.returnValue(of(mockContest));

        const polling$ = service
          .pollContestDetails(circleId, contestId, 5)
          .pipe(take(2));

        const expectedMarble = 'a 4ms (a|)';
        const expectedValues = {
          a: jasmine.objectContaining({
            id: contestId,
            totals: jasmine.objectContaining({
              byOption: jasmine.any(Map),
            }),
          }),
        };

        expectObservable(polling$).toBe(expectedMarble, expectedValues);
      });

      expect(service.getContest).toHaveBeenCalledTimes(2);
    });

    it('should stop polling when contest status becomes EXPIRED', () => {
      scheduler.run(({ expectObservable }) => {
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
          locked_at: '2024-01-02T00:00:00Z',
          duration: '1d',
        };

        const mockContestClosed: Contest = {
          ...mockContestOpen,
          status: 'EXPIRED',
        };

        let callCount = 0;
        spyOn(service, 'getContest').and.callFake(() => {
          callCount++;
          return of(callCount === 1 ? mockContestOpen : mockContestClosed);
        });

        const polling$ = service.pollContestDetails(circleId, contestId, 5);

        // Should emit OPEN at 0ms, then EXPIRED at 5ms and complete
        const expectedMarble = 'a 4ms (b|)';
        const expectedValues = {
          a: jasmine.objectContaining({ status: 'OPEN' }),
          b: jasmine.objectContaining({ status: 'EXPIRED' }),
        };

        expectObservable(polling$).toBe(expectedMarble, expectedValues);
      });
    });

    it('should stop polling when contest status becomes RESOLVED', () => {
      scheduler.run(({ expectObservable }) => {
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
          locked_at: '2024-01-02T00:00:00Z',
          duration: '1d',
        };

        const mockContestResolved: Contest = {
          ...mockContestLocked,
          status: 'RESOLVED',
          result_option_id: 1,
        };

        let callCount = 0;
        spyOn(service, 'getContest').and.callFake(() => {
          callCount++;
          return of(callCount === 1 ? mockContestLocked : mockContestResolved);
        });

        const polling$ = service.pollContestDetails(circleId, contestId, 5);

        // Should emit LOCKED at 0ms, then RESOLVED at 5ms and complete
        const expectedMarble = 'a 4ms (b|)';
        const expectedValues = {
          a: jasmine.objectContaining({ status: 'LOCKED' }),
          b: jasmine.objectContaining({ status: 'RESOLVED' }),
        };

        expectObservable(polling$).toBe(expectedMarble, expectedValues);
      });
    });
  });
});
