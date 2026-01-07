import { MonoTypeOperatorFunction, timer, throwError } from 'rxjs';
import { mergeMap, retryWhen } from 'rxjs/operators';

/**
 * RxJS operator for exponential backoff retry logic.
 * Retries a failed observable with exponentially increasing delays,
 * capped at a maximum delay and maximum number of attempts.
 *
 * @param baseMs - Base delay in milliseconds (e.g., 1000 for 1s initial delay)
 * @param maxMs - Maximum delay cap in milliseconds (e.g., 30000 for 30s cap)
 * @param maxRetries - Maximum number of retry attempts (e.g., 3)
 * @returns RxJS operator for retrying with exponential backoff
 *
 * @example
 * ```typescript
 * http.get('/api/data').pipe(
 *   exponentialBackoff(1000, 30000, 3),
 * ).subscribe(...);
 * ```
 */
export function exponentialBackoff<T>(
  baseMs: number,
  maxMs: number,
  maxRetries: number,
): MonoTypeOperatorFunction<T> {
  return retryWhen((errors) =>
    errors.pipe(
      mergeMap((error, index) => {
        const retryAttempt = index + 1;

        // Stop retrying after max attempts
        if (retryAttempt > maxRetries) {
          return throwError(() => error);
        }

        // Calculate exponential delay: base * 2^attempt, capped at maxMs
        const delayMs = Math.min(baseMs * Math.pow(2, index), maxMs);

        console.warn(
          `Retry attempt ${retryAttempt}/${maxRetries} after ${delayMs}ms`,
          error,
        );

        // Wait for the calculated delay before retrying
        return timer(delayMs);
      }),
    ),
  );
}
