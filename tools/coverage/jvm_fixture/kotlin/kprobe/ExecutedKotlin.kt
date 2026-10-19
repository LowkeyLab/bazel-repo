package kprobe
inline fun doubled(value: Int): Int {
  return value * 2
}

fun selectKotlin(take: Boolean): Int {
  return if (take) doubled(11) else 37
}
