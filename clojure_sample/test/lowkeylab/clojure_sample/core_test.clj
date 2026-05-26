(ns lowkeylab.clojure-sample.core-test
  (:require [clojure.test :refer [deftest is]]
            [lowkeylab.clojure-sample.core :as core]))

(deftest greeting-includes-name
  (is (= "Hello, Bazel from rules_clojure!"
         (core/greeting "Bazel"))))
