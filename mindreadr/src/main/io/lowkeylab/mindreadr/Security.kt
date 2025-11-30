package io.lowkeylab.mindreadr

import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.server.application.Application
import io.ktor.server.application.install
import io.ktor.server.plugins.cors.routing.CORS

fun Application.configureSecurity() {
    val config = environment.config
    val baseAllowedHosts = config.property("cors.allowedOrigins").getList()
    val allowedHosts = calculateAllowedOrigins(baseAllowedHosts, config.property("cors.extraAllowedOrigins").getString())
    install(CORS) {
        allowMethod(HttpMethod.Options)
        allowMethod(HttpMethod.Get)
        allowMethod(HttpMethod.Put)
        allowMethod(HttpMethod.Delete)
        allowMethod(HttpMethod.Patch)
        allowMethod(HttpMethod.Post)
        allowedHosts.forEach {
            allowHost(host = it, schemes = listOf("http", "https", "ws", "wss"))
        }
        allowHeader(HttpHeaders.ContentType)
    }
}

fun calculateAllowedOrigins(
    base: List<String>,
    extra: String,
): Set<String> {
    val extraAllowedHosts =
        extra
            .split(",")
            .map { it.trim() }
            .filter { it.isNotEmpty() }
    return (base + extraAllowedHosts).toSet()
}
