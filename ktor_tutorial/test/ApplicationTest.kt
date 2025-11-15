package com.example

import com.example.model.FakeTaskRepository
import com.example.model.Priority
import com.example.model.Task
import io.ktor.client.call.body
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.serialization.kotlinx.json.json
import io.ktor.server.testing.testApplication
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals

class ApplicationTest {
    @Test
    fun tasksCanBeFoundByPriority() =
        testApplication {
            application {
                val repository = FakeTaskRepository()
                configureSerialization(repository)
                configureRouting()
            }

            val client =
                createClient {
                    install(ContentNegotiation) {
                        json()
                    }
                }

            val response = client.get("/tasks/byPriority/Medium")
            val results = response.body<List<Task>>()

            assertEquals(HttpStatusCode.OK, response.status)

            val expectedTaskNames = listOf("gardening", "painting")
            val actualTaskNames = results.map(Task::name)
            assertContentEquals(expectedTaskNames, actualTaskNames)
        }

    @Test
    fun invalidPriorityProduces400() =
        testApplication {
            application {
                val repository = FakeTaskRepository()
                configureSerialization(repository)
                configureRouting()
            }
            val response = client.get("/tasks/byPriority/Invalid")
            assertEquals(HttpStatusCode.BadRequest, response.status)
        }

    @Test
    fun unusedPriorityProduces404() =
        testApplication {
            application {
                val repository = FakeTaskRepository()
                configureSerialization(repository)
                configureRouting()
            }

            val response = client.get("/tasks/byPriority/Vital")
            assertEquals(HttpStatusCode.NotFound, response.status)
        }

    @Test
    fun newTasksCanBeAdded() =
        testApplication {
            application {
                val repository = FakeTaskRepository()
                configureSerialization(repository)
                configureRouting()
            }

            val client =
                createClient {
                    install(ContentNegotiation) {
                        json()
                    }
                }

            val task = Task("swimming", "Go to the beach", Priority.Low)
            val response1 =
                client.post("/tasks") {
                    header(
                        HttpHeaders.ContentType,
                        ContentType.Application.Json,
                    )

                    setBody(task)
                }
            assertEquals(HttpStatusCode.NoContent, response1.status)

            val response2 = client.get("/tasks")
            assertEquals(HttpStatusCode.OK, response2.status)

            val taskNames =
                response2
                    .body<List<Task>>()
                    .map { it.name }

            assertContains(taskNames, "swimming")
        }
}
