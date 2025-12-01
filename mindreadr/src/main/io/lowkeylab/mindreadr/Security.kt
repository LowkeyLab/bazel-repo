package io.lowkeylab.mindreadr

import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.server.application.Application
import io.ktor.server.application.install
import io.ktor.server.plugins.cors.routing.CORS

fun Application.configureSecurity() {
  val config = environment.config
  val baseAllowedOrigins = config.property("cors.allowedOrigins").getList()
  val allowedOrigins =
      calculateAllowedOrigins(
          baseAllowedOrigins,
          config.property("cors.extraAllowedOrigins").getString(),
      )
  install(CORS) {
    allowMethod(HttpMethod.Options)
    allowMethod(HttpMethod.Get)
    allowMethod(HttpMethod.Put)
    allowMethod(HttpMethod.Delete)
    allowMethod(HttpMethod.Patch)
    allowMethod(HttpMethod.Post)
    allowedOrigins.forEach { allowHost(host = it, schemes = listOf("http", "https", "ws", "wss")) }
    allowHeader(HttpHeaders.ContentType)
  }
}

internal fun calculateAllowedOrigins(
    base: List<String>,
    extra: String,
): Set<String> {
  val trimmedBase = base.map { it.trim() }.filter { it.isNotEmpty() }
  val extraAllowedOrigins = extra.split(",").map { it.trim() }.filter { it.isNotEmpty() }
  return (trimmedBase + extraAllowedOrigins).toSet()
}
