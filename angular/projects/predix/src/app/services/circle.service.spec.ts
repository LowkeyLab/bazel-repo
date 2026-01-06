import { TestBed } from '@angular/core/testing';
import {
  HttpClientTestingModule,
  HttpTestingController,
} from '@angular/common/http/testing';
import { CircleService } from './circle.service';
import { environment } from '../../environments/environment';
import type { Circle } from '../models/circle.model';
import type { Contest } from '../models/contest.model';

describe('CircleService', () => {
  let service: CircleService;
  let httpMock: HttpTestingController;
  const apiUrl = `${environment.apiUrl}/protected/circles`;

  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [HttpClientTestingModule],
      providers: [CircleService],
    });

    service = TestBed.inject(CircleService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  describe('getCircleContests', () => {
    it('should fetch contests for a circle', () => {
      const circleId = 1;
      const mockContests: Contest[] = [
        {
          id: 1,
          circle_id: 1,
          creator_id: 1,
          question: 'What is 2+2?',
          options: [
            { id: 1, text: '3' },
            { id: 2, text: '4' },
          ],
          predictions: [],
          status: 'OPEN',
          result_option_id: undefined,
          min_stake: 10,
          total_pot: 0,
          house_rake: 0,
          created_at: '2024-01-01T00:00:00Z',
          closes_at: '2024-01-02T00:00:00Z',
          duration: '1d',
        },
      ];

      service.getCircleContests(circleId).subscribe((contests) => {
        expect(contests).toEqual(mockContests);
        expect(contests.length).toBe(1);
        expect(contests[0].question).toBe('What is 2+2?');
      });

      const req = httpMock.expectOne(`${apiUrl}/${circleId}/contests`);
      expect(req.request.method).toBe('GET');
      req.flush(mockContests);
    });

    it('should return an empty array when no contests exist', () => {
      const circleId = 1;
      const mockContests: Contest[] = [];

      service.getCircleContests(circleId).subscribe((contests) => {
        expect(contests).toEqual([]);
        expect(contests.length).toBe(0);
      });

      const req = httpMock.expectOne(`${apiUrl}/${circleId}/contests`);
      expect(req.request.method).toBe('GET');
      req.flush(mockContests);
    });

    it('should handle multiple contests', () => {
      const circleId = 1;
      const mockContests: Contest[] = [
        {
          id: 1,
          circle_id: 1,
          creator_id: 1,
          question: 'Question 1?',
          options: [
            { id: 1, text: 'Option A' },
            { id: 2, text: 'Option B' },
          ],
          predictions: [
            {
              user_id: 2,
              option_id: 1,
              clout: 100,
              timestamp: '2024-01-01T12:00:00Z',
            },
          ],
          status: 'OPEN',
          result_option_id: undefined,
          min_stake: 10,
          total_pot: 100,
          house_rake: 10,
          created_at: '2024-01-01T00:00:00Z',
          closes_at: '2024-01-02T00:00:00Z',
          duration: '1d',
        },
        {
          id: 2,
          circle_id: 1,
          creator_id: 1,
          question: 'Question 2?',
          options: [
            { id: 1, text: 'Yes' },
            { id: 2, text: 'No' },
          ],
          predictions: [],
          status: 'CLOSED',
          result_option_id: undefined,
          min_stake: 10,
          total_pot: 200,
          house_rake: 20,
          created_at: '2024-01-01T00:00:00Z',
          closes_at: '2024-01-02T00:00:00Z',
          duration: '1d',
        },
      ];

      service.getCircleContests(circleId).subscribe((contests) => {
        expect(contests.length).toBe(2);
        expect(contests[0].id).toBe(1);
        expect(contests[1].id).toBe(2);
      });

      const req = httpMock.expectOne(`${apiUrl}/${circleId}/contests`);
      req.flush(mockContests);
    });

    it('should handle server error gracefully', () => {
      const circleId = 1;

      service.getCircleContests(circleId).subscribe({
        next: () => fail('should have failed'),
        error: (error) => {
          expect(error.status).toBe(500);
        },
      });

      const req = httpMock.expectOne(`${apiUrl}/${circleId}/contests`);
      req.flush('Server error', {
        status: 500,
        statusText: 'Internal Server Error',
      });
    });
  });

  describe('other methods', () => {
    it('should create a circle', () => {
      const mockCircle: Circle = {
        id: 1,
        name: 'Study Group',
        created_at: '2024-01-01T00:00:00Z',
        members: [],
      };

      service.createCircle({ name: 'Study Group' }).subscribe((circle) => {
        expect(circle).toEqual(mockCircle);
      });

      const req = httpMock.expectOne(apiUrl);
      expect(req.request.method).toBe('POST');
      req.flush(mockCircle);
    });

    it('should list user circles', () => {
      const mockCircles: Circle[] = [
        {
          id: 1,
          name: 'Circle 1',
          created_at: '2024-01-01T00:00:00Z',
          members: [],
        },
      ];

      service.listUserCircles().subscribe((circles) => {
        expect(circles).toEqual(mockCircles);
      });

      const req = httpMock.expectOne(apiUrl);
      expect(req.request.method).toBe('GET');
      req.flush(mockCircles);
    });
  });
});
