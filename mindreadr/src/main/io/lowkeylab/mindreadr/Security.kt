package io.lowkeylab.mindreadr

import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.server.application.Application
import io.ktor.server.application.install
import io.ktor.server.plugins.cors.routing.CORS

fun Application.configureSecurity() {
    val config = environment.config
    val baseAllowedHosts = config.property("cors.allowedHosts").getList()
    val extraAllowedHosts =
        config
            .property("cors.extraAllowedOrigins")
            .getString()
            .split(",")
            .toSet()
    val allowedHosts = (baseAllowedHosts + extraAllowedHosts).filter { it.isNotBlank() }
    install(CORS) {
        allowMethod(HttpMethod.Options)
        allowMethod(HttpMethod.Get)
        allowMethod(HttpMethod.Put)
        allowMethod(HttpMethod.Delete)
        allowMethod(HttpMethod.Patch)
        allowMethod(HttpMethod.Post)
        allowedHosts.forEach {
            allowHost(it, schemes = listOf("http", "https", "ws", "wss"))
        }
        allowHeader(HttpHeaders.ContentType)
    }
}
