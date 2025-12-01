package io.lowkeylab.mindreadr

import kotlin.test.Test
import kotlin.test.assertEquals

class CalculateAllowedOriginsTest {
  @Test
  fun `merges base and extra origins removing blanks and duplicates`() {
    val base = listOf("example.com", "api.example.com")
    val extra = "api.example.com, other.com , , another.com"

    val result = calculateAllowedOrigins(base, extra)

    assertEquals(
        setOf(
            "example.com",
            "api.example.com",
            "other.com",
            "another.com",
        ),
        result,
    )
  }

  @Test
  fun `handles empty extra string`() {
    val base = listOf("foo.com")
    val extra = "   "

    val result = calculateAllowedOrigins(base, extra)

    assertEquals(setOf("foo.com"), result)
  }

  @Test
  fun `handles empty base list`() {
    val base = emptyList<String>()
    val extra = "bar.com,baz.com"

    val result = calculateAllowedOrigins(base, extra)

    assertEquals(setOf("bar.com", "baz.com"), result)
  }

  @Test
  fun `trims whitespace around hosts`() {
    val base = listOf(" one.com ", "two.com")
    val extra = " three.com , four.com"

    val result = calculateAllowedOrigins(base, extra)

    assertEquals(
        setOf("one.com", "two.com", "three.com", "four.com"),
        result,
    )
  }
}
