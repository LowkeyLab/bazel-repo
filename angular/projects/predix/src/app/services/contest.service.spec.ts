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
import { AuthService } from './auth.service';

describe('ContestService', () => {
  let service: ContestService;
  let httpMock: HttpTestingController;
  const apiUrl = `${environment.apiUrl}/protected`;
  const mockAuthService = {
    token: jasmine.createSpy('token').and.returnValue('test-token'),
  };

  beforeEach(() => {
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
});
