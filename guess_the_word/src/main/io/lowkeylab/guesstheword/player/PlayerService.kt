package io.lowkeylab.guesstheword.player

class PlayerService(
    private val adjectiveList: List<String>,
    private val nounList: List<String>,
    private val playerRepository: PlayerRepository,
) {
    suspend fun createPlayer(): Player {
        val randomAdjective = adjectiveList.random()
        val randomNoun = nounList.random()
        val playerName = "$randomAdjective $randomNoun"
        return playerRepository.createPlayer(playerName)
    }

    suspend fun getPlayerById(playerId: PlayerId): Player? = playerRepository.getPlayerById(playerId)
}

class PlayerServiceFactory(
    private val playerRepository: PlayerRepository,
) {
    fun fromClasspath(): PlayerService {
        val adjectiveList =
            this::class.java
                .getResourceAsStream("/adjectives.txt")!!
                .bufferedReader()
                .readLines()
        val nounList =
            this::class.java
                .getResourceAsStream("/nouns.txt")!!
                .bufferedReader()
                .readLines()
        return PlayerService(
            adjectiveList = adjectiveList,
            nounList = nounList,
            playerRepository = playerRepository,
        )
    }
}
