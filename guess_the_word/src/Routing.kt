package io.lowkeylabs

import io.ktor.http.ContentType
import io.ktor.server.application.Application
import io.ktor.server.response.respondText
import io.ktor.server.routing.get
import io.ktor.server.routing.routing
import io.lowkeylabs.model.tasks
import io.lowkeylabs.model.tasksAsTable

fun Application.configureRouting() {
    routing {
        get("/tasks") {
            call.respondText(
                contentType = ContentType.parse("text/html"),
                text = tasks.tasksAsTable(),
            )
        }
    }
}
