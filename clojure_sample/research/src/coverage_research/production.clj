(ns coverage-research.production)

(def initialization-marker (atom 0))
(swap! initialization-marker inc)

(defmacro twice [value]
  `(+ ~value ~value))

(defn calculate [value]
  (let [next-value (+ value 1)]
    (twice next-value)))
