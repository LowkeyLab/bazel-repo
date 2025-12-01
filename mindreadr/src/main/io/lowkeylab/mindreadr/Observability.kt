package io.lowkeylab.mindreadr

import io.ktor.http.HttpHeaders
import io.ktor.server.application.Application
import io.ktor.server.application.install
import io.ktor.server.plugins.callid.CallId
import io.ktor.server.plugins.callid.callIdMdc
import io.ktor.server.plugins.callid.generate
import io.ktor.server.plugins.calllogging.CallLogging

fun Application.configureObservability() {
  install(CallId) {
    header(HttpHeaders.XRequestId)
    generate(10, "abcde12345")
  }

  install(CallLogging) { callIdMdc("call-id") }
}
