package kprobe
private val initializer: Int = error("Baseline executed Kotlin initializer")

fun untouchedKotlin(take: Boolean): Int {
  return if (take) 47 else 53
}
