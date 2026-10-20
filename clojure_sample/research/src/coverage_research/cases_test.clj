(ns coverage-research.cases-test
  (:require [clojure.test :refer [deftest is]]
            [coverage-research.production :as production]
            [coverage-research.helper :as helper]))

(deftest controlled-result
  (is (= 8 (production/calculate 3)))
  (is (= 7 (helper/adjust 5))))
