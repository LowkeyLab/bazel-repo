package io.lowkeylab.mindrdr.app

import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.plugins.websocket.webSocket
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
import kotlinx.coroutines.delay
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
            val client =
                createClient {
                    install(WebSockets)
                }

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

            val client =
                createClient {
                    install(WebSockets)
                }

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
            val client =
                createClient {
                    install(WebSockets)
                }

            // Fill the game with 2 players
            client.webSocket("/games/${game.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                client.webSocket("/games/${game.id.id}/live") {
                    incoming.receive() // PlayerJoined
                    incoming.receive() // GameState
                    incoming.receive() // GameState for client 1

                    // Try to add a third player
                    client.webSocket("/games/${game.id.id}/live") {
                        val reason = closeReason.await()
                        assertTrue(reason != null, "Session should be closed for full game")
                    }
                }
            }
        }

    @Test
    fun `two clients both receive GameState broadcasts`() =
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
            val client =
                createClient {
                    install(WebSockets)
                }

            client.webSocket("/games/${game.id.id}/live") {
                // Client 1 joins
                val frame1 = incoming.receive()
                assertIs<Frame.Text>(frame1)
                val playerJoined1 = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                assertIs<OutgoingMessage.PlayerJoined>(playerJoined1)
                assertEquals("player1", playerJoined1.player.name.name)

                val frame2 = incoming.receive()
                assertIs<Frame.Text>(frame2)
                val state1 = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                assertIs<OutgoingMessage.GameState>(state1)
                assertEquals(1, state1.game.getPlayerCount())

                client.webSocket("/games/${game.id.id}/live") {
                    // Client 2 joins
                    val frame3 = incoming.receive()
                    assertIs<Frame.Text>(frame3)
                    val playerJoined2 = Json.decodeFromString(OutgoingMessage.serializer(), frame3.readText())
                    assertIs<OutgoingMessage.PlayerJoined>(playerJoined2)
                    assertEquals("player2", playerJoined2.player.name.name)

                    val frame4 = incoming.receive()
                    assertIs<Frame.Text>(frame4)
                    val state2 = Json.decodeFromString(OutgoingMessage.serializer(), frame4.readText())
                    assertIs<OutgoingMessage.GameState>(state2)
                    assertEquals(2, state2.game.getPlayerCount())

                    // Client 1 should also receive updated state
                    val frame5 = incoming.receive()
                    assertIs<Frame.Text>(frame5)
                    val state3 = Json.decodeFromString(OutgoingMessage.serializer(), frame5.readText())
                    assertIs<OutgoingMessage.GameState>(state3)
                    assertEquals(2, state3.game.getPlayerCount())
                }
            }
        }

    @Test
    fun `round progression when clients submit different guesses`() =
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
            val client =
                createClient {
                    install(WebSockets)
                }

            client.webSocket("/games/${game.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                client.webSocket("/games/${game.id.id}/live") {
                    incoming.receive() // PlayerJoined
                    incoming.receive() // GameState
                    incoming.receive() // GameState for client 1

                    // Submit different guesses
                    val guess1 = IncomingMessage.SubmitGuess("apple")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess1)))

                    val guess2 = IncomingMessage.SubmitGuess("banana")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess2)))

                    // Both clients should receive updated state
                    val frame1 = incoming.receive()
                    assertIs<Frame.Text>(frame1)
                    val state1 = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                    assertIs<OutgoingMessage.GameState>(state1)
                    assertEquals(2, state1.game.getRoundCount()) // Round progressed

                    val frame2 = incoming.receive()
                    assertIs<Frame.Text>(frame2)
                    val state2 = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                    assertIs<OutgoingMessage.GameState>(state2)
                    assertEquals(2, state2.game.getRoundCount())
                }
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
            val client =
                createClient {
                    install(WebSockets)
                }

            client.webSocket("/games/${game.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                client.webSocket("/games/${game.id.id}/live") {
                    incoming.receive() // PlayerJoined
                    incoming.receive() // GameState
                    incoming.receive() // GameState for client 1

                    // Both submit the same guess
                    val guess1 = IncomingMessage.SubmitGuess("winner")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess1)))

                    val guess2 = IncomingMessage.SubmitGuess("winner")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess2)))

                    // Both should receive GameState showing game ended
                    val frame1 = incoming.receive()
                    assertIs<Frame.Text>(frame1)
                    val state1 = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                    assertIs<OutgoingMessage.GameState>(state1)
                    assertTrue(state1.game.hasEnded())

                    val frame2 = incoming.receive()
                    assertIs<Frame.Text>(frame2)
                    val state2 = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                    assertIs<OutgoingMessage.GameState>(state2)
                    assertTrue(state2.game.hasEnded())

                    // Both should receive GameTerminated
                    val frame3 = incoming.receive()
                    assertIs<Frame.Text>(frame3)
                    val terminated1 = Json.decodeFromString(OutgoingMessage.serializer(), frame3.readText())
                    assertIs<OutgoingMessage.GameTerminated>(terminated1)

                    val frame4 = incoming.receive()
                    assertIs<Frame.Text>(frame4)
                    val terminated2 = Json.decodeFromString(OutgoingMessage.serializer(), frame4.readText())
                    assertIs<OutgoingMessage.GameTerminated>(terminated2)
                }
            }
        }

    @Test
    fun `concurrent guess submissions with delay handle race conditions`() =
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
            val client =
                createClient {
                    install(WebSockets)
                }

            client.webSocket("/games/${game.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                client.webSocket("/games/${game.id.id}/live") {
                    incoming.receive() // PlayerJoined
                    incoming.receive() // GameState
                    incoming.receive() // GameState for client 1

                    // Submit guesses with small delay
                    val guess1 = IncomingMessage.SubmitGuess("apple")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess1)))
                    delay(50) // Short delay to test race conditions

                    val guess2 = IncomingMessage.SubmitGuess("banana")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess2)))

                    delay(100) // Give time for messages to propagate

                    // Both should receive state updates
                    val frame1 = incoming.receive()
                    assertIs<Frame.Text>(frame1)
                    val state1 = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                    assertIs<OutgoingMessage.GameState>(state1)
                    assertEquals(2, state1.game.getPlayerCount())

                    val frame2 = incoming.receive()
                    assertIs<Frame.Text>(frame2)
                    val state2 = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                    assertIs<OutgoingMessage.GameState>(state2)
                    assertEquals(2, state2.game.getPlayerCount())
                }
            }
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
            val client =
                createClient {
                    install(WebSockets)
                }

            client.webSocket("/games/${game.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                client.webSocket("/games/${game.id.id}/live") {
                    incoming.receive() // PlayerJoined
                    incoming.receive() // GameState
                    incoming.receive() // GameState for client 1

                    // Client 2 disconnects
                    close(CloseReason(CloseReason.Codes.NORMAL, "Test disconnect"))
                }

                // Client 1 should receive GameTerminated
                delay(100) // Give time for disconnection to process
                val frame = incoming.receive()
                assertIs<Frame.Text>(frame)
                val terminated = Json.decodeFromString(OutgoingMessage.serializer(), frame.readText())
                assertIs<OutgoingMessage.GameTerminated>(terminated)
                assertTrue(terminated.reason.contains("left"))
            }
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
            val client =
                createClient {
                    install(WebSockets)
                }

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
                send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess)))
            }
        }

    @Test
    fun `two player game flow with round progression`() =
        testApplication {
            val playerFactory = TestPlayerFactory()
            val gameRepository = InMemoryGameRepository()
            val gameService = GameService(playerFactory, gameRepository)

            application {
                configureSockets()
                configureSerialization()
                configureGames(gameService)
            }

            val createdGame = gameService.createGame()
            val client =
                createClient {
                    install(WebSockets)
                }

            client.webSocket("/games/${createdGame.id.id}/live") {
                incoming.receive() // PlayerJoined
                incoming.receive() // GameState

                client.webSocket("/games/${createdGame.id.id}/live") {
                    incoming.receive() // PlayerJoined
                    incoming.receive() // GameState
                    incoming.receive() // GameState for client 1

                    // Game is now in progress with 2 players
                    val guess1 = IncomingMessage.SubmitGuess("cat")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess1)))

                    val guess2 = IncomingMessage.SubmitGuess("dog")
                    send(Frame.Text(Json.encodeToString(IncomingMessage.serializer(), guess2)))

                    // Verify state updates
                    val frame1 = incoming.receive()
                    assertIs<Frame.Text>(frame1)
                    val state1 = Json.decodeFromString(OutgoingMessage.serializer(), frame1.readText())
                    assertIs<OutgoingMessage.GameState>(state1)
                    assertEquals(2, state1.game.getPlayerCount())

                    val frame2 = incoming.receive()
                    assertIs<Frame.Text>(frame2)
                    val state2 = Json.decodeFromString(OutgoingMessage.serializer(), frame2.readText())
                    assertIs<OutgoingMessage.GameState>(state2)
                    assertEquals(2, state2.game.getPlayerCount())
                }
            }
        }
}
