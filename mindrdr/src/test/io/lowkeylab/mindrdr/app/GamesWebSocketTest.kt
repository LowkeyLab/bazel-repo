package io.lowkeylab.mindrdr.app

import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.plugins.websocket.converter
import io.ktor.client.plugins.websocket.sendSerialized
import io.ktor.client.plugins.websocket.webSocket
import io.ktor.serialization.deserialize
import io.ktor.serialization.kotlinx.KotlinxWebsocketSerializationConverter
import io.ktor.server.testing.ApplicationTestBuilder
import io.ktor.server.testing.testApplication
import io.ktor.websocket.CloseReason
import io.ktor.websocket.Frame
import io.ktor.websocket.close
import io.ktor.websocket.readText
import io.lowkeylab.mindrdr.IncomingMessage
import io.lowkeylab.mindrdr.OutgoingMessage
import io.lowkeylab.mindrdr.configureGames
import io.lowkeylab.mindrdr.configureSerialization
import io.lowkeylab.mindrdr.configureSockets
import io.lowkeylab.mindrdr.game.GameService
import io.lowkeylab.mindrdr.game.InMemoryGameRepository
import io.lowkeylab.mindrdr.game.Player
import io.lowkeylab.mindrdr.game.PlayerFactory
import io.lowkeylab.mindrdr.game.PlayerName
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class GamesWebSocketTest {
    // Inner class for test player factory with incrementing names
    private class TestPlayerFactory : PlayerFactory {
        private val counter = AtomicInteger(0)

        override fun create(): Player {
            val count = counter.incrementAndGet()
            return Player(PlayerName("player$count"))
        }

        override fun removeName(name: PlayerName) {
            // No-op for tests
        }
    }

    @Test
    fun `single client connection receives PlayerJoined then GameState in order`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val game = gameService.createGame()
            val client = createClient()

            client.webSocket("/games/${game.id.id}/live") {
                // Assert strict message order: PlayerJoined first
                val frame1 = incoming.receive()
                assertIs<Frame.Text>(frame1)
                val playerJoined = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                assertIs<OutgoingMessage.PlayerJoined>(playerJoined)
                assertEquals("player1", playerJoined.player.name.name)

                // Then GameState
                val frame2 = incoming.receive()
                assertIs<Frame.Text>(frame2)
                val gameState = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                assertIs<OutgoingMessage.GameState>(gameState)
                assertEquals(1, gameState.game.getPlayerCount())
            }
        }

    @Test
    fun `connection to non-existent game closes session`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val client = createClient()

            client.webSocket("/games/non-existent-game/live") {
                val reason = closeReason.await()
                assertTrue(reason != null, "Session should be closed")
            }
        }

    @Test
    fun `joining full game closes session`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val game = gameService.createGame()
            val client = createClient()

            // Fill the game with 2 players
            client.webSocket("/games/${game.id.id}/live") {}

            client.webSocket("/games/${game.id.id}/live") {}

            // Try to add a third player
            client.webSocket("/games/${game.id.id}/live") {
                val reason = closeReason.await()
                assertTrue(reason != null, "Session should be closed for full game")
            }
        }

    @Test
    fun `game completion when all submit identical guess`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val game = gameService.createGame()
            val client = createClient()

            var clientOneMessages = emptyList<OutgoingMessage>()
            var clientTwoMessages = emptyList<OutgoingMessage>()

            coroutineScope {
                val clientOne =
                    launch {
                        client.webSocket("/games/${game.id.id}/live") {
                            sendSerialized<IncomingMessage>(IncomingMessage.SubmitGuess("apple"))

                            consumeMessagesAsFlow()
                                .collect { messages -> clientOneMessages = messages }
                        }
                    }

                val clientTwo =
                    launch {
                        client.webSocket("/games/${game.id.id}/live") {
                            sendSerialized<IncomingMessage>(IncomingMessage.SubmitGuess("apple"))

                            consumeMessagesAsFlow()
                                .collect { messages -> clientTwoMessages = messages }
                        }
                    }

                clientOne.join()
                clientTwo.join()
            }

            val finalMessageClientOne = clientOneMessages.last()
            val finalMessageClientTwo = clientTwoMessages.last()
            assertIs<OutgoingMessage.GameTerminated>(finalMessageClientOne)
            assertIs<OutgoingMessage.GameTerminated>(finalMessageClientTwo)
            assertTrue(finalMessageClientOne.reason.contains("completed"))
            assertTrue(finalMessageClientTwo.reason.contains("completed"))
        }

    @Test
    fun `player disconnection broadcasts GameTerminated to remaining clients`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val game = gameService.createGame()
            val client = createClient()

            var clientOneMessages = emptyList<OutgoingMessage>()

            client.webSocket("/games/${game.id.id}/live") {
                client.webSocket("/games/${game.id.id}/live") {
                    // Client 2 disconnects
                    close(CloseReason(CloseReason.Codes.NORMAL, "Test disconnect"))
                }

                consumeMessagesAsFlow()
                    .collect { messages -> clientOneMessages = messages }
            }

            val finalMessageClientOne = clientOneMessages.last()
            assertIs<OutgoingMessage.GameTerminated>(finalMessageClientOne)
            assertTrue(finalMessageClientOne.reason.contains("left"))
        }

    @Test
    fun `invalid JSON message returns Error and continues`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val game = gameService.createGame()
            val client = createClient()

            client.webSocket("/games/${game.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                // Send invalid JSON
                send(Frame.Text("{invalid json}"))

                // Should receive Error message
                val frame = incoming.receive()
                assertIs<Frame.Text>(frame)
                val error = Json.decodeFromString(OutgoingMessage.serializer(), frame.readText())
                assertIs<OutgoingMessage.Error>(error)
                assertTrue(error.message.contains("Invalid message format"))

                // Session should still be open, can continue
                val guess = IncomingMessage.SubmitGuess("test")
                sendSerialized<IncomingMessage>(guess)
            }
        }

    private fun ApplicationTestBuilder.createClient(): HttpClient =
        createClient {
            install(WebSockets) {
                contentConverter = KotlinxWebsocketSerializationConverter(Json)
            }
        }

    private fun DefaultClientWebSocketSession.consumeMessagesAsFlow(): Flow<List<OutgoingMessage>> =
        incoming
            .consumeAsFlow()
            .map {
                converter!!.deserialize<OutgoingMessage>(it)
            }.scan(emptyList<OutgoingMessage>()) { acc, message -> acc + message }
}
