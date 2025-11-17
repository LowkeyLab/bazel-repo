package io.lowkeylab.mindrdr

import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.server.application.Application
import io.ktor.server.application.install
import io.ktor.server.plugins.cors.routing.CORS
import io.ktor.server.sessions.SessionStorageMemory
import io.ktor.server.sessions.Sessions
import io.ktor.server.sessions.cookie
import io.lowkeylab.mindrdr.player.PlayerId
import kotlinx.serialization.Serializable

private const val SESSION_HEADER_NAME = "player_session"

fun Application.configureSecurity() {
    install(CORS) {
        allowMethod(HttpMethod.Options)
        allowMethod(HttpMethod.Put)
        allowMethod(HttpMethod.Delete)
        allowMethod(HttpMethod.Patch)
        allowHeader(HttpHeaders.Authorization)
        allowHeader(SESSION_HEADER_NAME)
        exposeHeader(SESSION_HEADER_NAME)
        anyHost() // @TODO: Don't do this in production if possible. Try to limit it.
    }

    install(Sessions) {
        cookie<PlayerSession>(SESSION_HEADER_NAME, SessionStorageMemory()) {
            cookie.path = "/"
            cookie.maxAgeInSeconds = 60 * 60 * 24 * 7 // 1 week
        }
    }
}

@Serializable
data class PlayerSession(
    val id: PlayerId,
    val name: String,
)
