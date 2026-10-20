(ns coverage-research.disconnected)

(defn untouched [value]
  (+ value 700))

(when-let [path (System/getProperty "coverage.research.marker")]
  (spit path "initialized\n" :append true))
