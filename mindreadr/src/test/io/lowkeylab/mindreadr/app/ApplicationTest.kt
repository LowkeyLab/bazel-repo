package io.lowkeylab.mindreadr.app

import io.ktor.client.request.get
import io.ktor.http.HttpStatusCode
import io.ktor.server.testing.testApplication
import io.lowkeylab.mindreadr.configureRouting
import kotlin.test.Test
import kotlin.test.assertEquals

class ApplicationTest {
  @Test
  fun testRoot() = testApplication {
    application { configureRouting() }
    client.get("/").apply { assertEquals(HttpStatusCode.Companion.OK, status) }
  }
}
