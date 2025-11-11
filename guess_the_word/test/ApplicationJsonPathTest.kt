package io.lowkeylabs

import com.jayway.jsonpath.DocumentContext
import com.jayway.jsonpath.JsonPath
import io.ktor.client.HttpClient
import io.ktor.client.request.accept
import io.ktor.client.request.get
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.server.testing.testApplication
import io.lowkeylabs.model.Priority
import kotlin.test.Test
import kotlin.test.assertEquals

class ApplicationJsonPathTest {
    @Test
    fun tasksCanBeFound() =
        testApplication {
            application {
                module()
            }
            val jsonDoc = client.getAsJsonPath("/tasks")

            val result: List<String> = jsonDoc.read("$[*].name")
            assertEquals("cleaning", result[0])
            assertEquals("gardening", result[1])
            assertEquals("shopping", result[2])
        }

    @Test
    fun tasksCanBeFoundByPriority() =
        testApplication {
            application {
                module()
            }
            val priority = Priority.Medium
            val jsonDoc = client.getAsJsonPath("/tasks/byPriority/$priority")

            val result: List<String> =
                jsonDoc.read("$[?(@.priority == '$priority')].name")
            assertEquals(2, result.size)

            assertEquals("gardening", result[0])
            assertEquals("painting", result[1])
        }

    suspend fun HttpClient.getAsJsonPath(url: String): DocumentContext {
        val response =
            this.get(url) {
                accept(ContentType.Application.Json)
            }
        return JsonPath.parse(response.bodyAsText())
    }
}
