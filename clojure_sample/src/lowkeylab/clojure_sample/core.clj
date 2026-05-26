(ns lowkeylab.clojure-sample.core
  "Small Clojure sample used to validate rules_clojure integration.")

(defn greeting
  "Build a friendly greeting for name."
  [name]
  (str "Hello, " name " from rules_clojure!"))

(defn -main
  [& args]
  (let [name (or (first args) "Bazel")]
    (println (greeting name))))
