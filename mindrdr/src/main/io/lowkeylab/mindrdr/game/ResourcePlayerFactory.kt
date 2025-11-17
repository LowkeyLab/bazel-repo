package io.lowkeylab.mindrdr.game

class ResourcePlayerFactory(
    adjectiveFilePath: String,
    nounFilePath: String,
) : PlayerFactory {
    private val existingNames: MutableSet<String> = mutableSetOf()
    private val adjectives =
        ClassLoader
            .getSystemClassLoader()
            .getResourceAsStream(adjectiveFilePath)
            ?.bufferedReader()
            ?.readLines()
            ?: throw IllegalStateException("Could not load adjectives from $adjectiveFilePath")

    private val nouns =
        ClassLoader
            .getSystemClassLoader()
            .getResourceAsStream(nounFilePath)
            ?.bufferedReader()
            ?.readLines()
            ?: throw IllegalStateException("Could not load nouns from $nounFilePath")

    override fun create(): Player {
        var name: String
        do {
            val adjective = adjectives.random()
            val noun = nouns.random()
            name = "${adjective}_$noun"
        } while (existingNames.contains(name))
        existingNames.add(name)
        return Player(name = PlayerName(name))
    }

    override fun removeName(name: PlayerName) {
        existingNames.remove(name.name)
    }
}
