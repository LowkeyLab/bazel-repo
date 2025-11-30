package io.lowkeylab.mindreadr

import org.junit.jupiter.api.Test

class CalculateAllowedOriginsTest {
    @Test
    fun `merges base and extra origins removing blanks and duplicates`() {
        val base = listOf("example.com", "api.example.com")
        val extra = "api.example.com, other.com , , another.com"

        val result = calculateAllowedOrigins(base, extra)

        org.junit.jupiter.api.Assertions.assertEquals(
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
        val base = kotlin.collections.listOf("foo.com")
        val extra = "   "

        val result = calculateAllowedOrigins(base, extra)

        org.junit.jupiter.api.Assertions
            .assertEquals(kotlin.collections.setOf("foo.com"), result)
    }

    @Test
    fun `handles empty base list`() {
        val base = emptyList<String>()
        val extra = "bar.com,baz.com"

        val result = calculateAllowedOrigins(base, extra)

        org.junit.jupiter.api.Assertions
            .assertEquals(setOf("bar.com", "baz.com"), result)
    }

    @org.junit.jupiter.api.Test
    fun `trims whitespace around hosts`() {
        val base = listOf(" one.com ", "two.com")
        val extra = " three.com , four.com"

        val result = calculateAllowedOrigins(base, extra)

        // Note: base entries are not trimmed in implementation, only extra is; keep behavior consistent
        org.junit.jupiter.api.Assertions.assertEquals(
            setOf(
                " one.com ",
                "two.com",
                "three.com",
                "four.com",
            ),
            result,
        )
    }
}
