package coverage.fixture;

/** A single-return method with one JaCoCo probe for synthetic profile fixtures. */
public interface Marker {
  default int value() {
    return 7;
  }
}
