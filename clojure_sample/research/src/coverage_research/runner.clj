(ns coverage-research.runner
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :as test]
            [cloverage.coverage :as coverage]
            [cloverage.report :as report]
            [cloverage.report.lcov :as lcov]
            [cloverage.source :as source]
            [cloverage.instrument :as instrument])
  (:import [java.lang ProcessHandle]
           [java.util.jar JarFile]
           [clojure.asm ClassReader ClassVisitor MethodVisitor Opcodes]
           [java.security MessageDigest]
           [java.util.concurrent TimeUnit]
           [java.util.concurrent.atomic AtomicInteger]))

(def sources
  (array-map
   'lowkeylab.clojure-sample.core "clojure_sample/src/lowkeylab/clojure_sample/core.clj"
   'coverage-research.production "clojure_sample/research/src/coverage_research/production.clj"
   'coverage-research.helper "clojure_sample/research/src/coverage_research/helper.clj"
   'coverage-research.disconnected "clojure_sample/research/src/coverage_research/disconnected.clj"
   'coverage-research.child "clojure_sample/research/src/coverage_research/child.clj"
   'lowkeylab.clojure-sample.core-test "clojure_sample/test/lowkeylab/clojure_sample/core_test.clj"
   'coverage-research.cases-test "clojure_sample/research/src/coverage_research/cases_test.clj"))

(defn save-json [file value]
  (spit file (json/write-str value :escape-slash false)))

(defn sha256-bytes [input-bytes]
  (format "%064x" (BigInteger. 1 (.digest (MessageDigest/getInstance "SHA-256") input-bytes))))

(defn sha256 [text]
  (sha256-bytes (.getBytes text "UTF-8")))

(defn preserve-inputs [directory aot-resources]
  (let [classpath (mapv (fn [path]
                          (let [file (io/file path)]
                            {:path path :isFile (.isFile file)
                             :sha256 (when (.isFile file)
                                       (sha256-bytes (java.nio.file.Files/readAllBytes (.toPath file))))}))
                        (str/split (System/getProperty "java.class.path") #":"))
        aot (mapv (fn [resource]
                    (let [file (io/file directory "aot-inputs" resource)]
                      (.mkdirs (.getParentFile file))
                      (with-open [input (io/input-stream (io/resource resource))]
                        (io/copy input file))
                      {:resource resource :sha256 (sha256-bytes (java.nio.file.Files/readAllBytes (.toPath file)))}))
                  aot-resources)]
    (save-json (io/file directory "input-inventory.json")
               {:classpath classpath :aotResources aot
                :clojureCodeSource (str (-> (Class/forName "clojure.lang.RT") .getProtectionDomain .getCodeSource .getLocation))})))

(defn check [condition message]
  (when-not condition (throw (ex-info message {}))))

(defn reset-hits []
  (doseq [form @coverage/*covered*]
    (.set ^AtomicInteger (:hits form) 0)))

(defn snapshot [directory stage mapping]
  (let [forms (coverage/covered)
        expanded (vec (report/gather-stats forms))
        records (mapv (fn [idx form]
                        {:index idx :file (:file form)
                         :canonicalFile (get mapping (:file form))
                         :line (:line form) :lib (str (:lib form))
                         :tracked (:tracked form) :hits (:hits form) :covered (:covered form)
                         :form (binding [*print-meta* true] (pr-str (:form form)))
                         :fullForm (binding [*print-meta* true] (pr-str (:full-form form)))})
                      (range) forms)]
    (check (every? :canonicalFile records) "A tracked form has no canonical source identity.")
    (save-json (io/file directory (str stage ".forms.json")) records)
    (spit (io/file directory (str stage ".forms.edn"))
          (binding [*print-meta* true] (pr-str forms)))
    (spit (io/file directory (str stage ".raw.lcov"))
          (with-out-str (lcov/write-lcov-report expanded)))
    (spit (io/file directory (str stage ".lcov"))
          (with-redefs [source/source-file-path
                        (fn [resource]
                          (or (get mapping resource)
                              (throw (ex-info "An exported source has no canonical identity." {:resource resource}))))]
            (with-out-str (lcov/write-lcov-report expanded))))
    records))

(defn lines [text]
  (check (seq text) "The LCOV artifact is empty.")
  (reduce (fn [{:keys [file] :as state} line]
            (cond
              (str/starts-with? line "SF:") (assoc state :file (subs line 3))
              (str/starts-with? line "DA:")
              (let [[_ n hits] (re-matches #"DA:([0-9]+),([0-9]+)" line)]
                (check (and file n hits) "An LCOV line record is invalid.")
                (assoc-in state [:records file (Long/parseLong n)] (Long/parseLong hits)))
              (or (str/starts-with? line "TN:") (str/starts-with? line "LF:")
                  (str/starts-with? line "LH:") (= line "end_of_record") (str/blank? line)) state
              :else (throw (ex-info "An LCOV record is invalid." {:line line}))))
          {:records {}} (str/split-lines text)))

(defn hit [records file line]
  (get-in records [:records file line] 0))

(defn run-child [mode directory]
  (.mkdirs (io/file directory))
  (let [java (str (System/getProperty "java.home") "/bin/java")
        command [java "-Xmx512m" "-cp" (System/getProperty "java.class.path")
                 (str "-Dcoverage.research.marker=" (io/file directory "initialization-marker.txt"))
                 "clojure.main" "-m" "coverage-research.runner" mode (str directory)]
        builder (ProcessBuilder. ^java.util.List command)
        _ (.redirectOutput builder (io/file directory "stdout.txt"))
        _ (.redirectError builder (io/file directory "stderr.txt"))
        process (.start builder)
        completed (.waitFor process 60 TimeUnit/SECONDS)]
    (when-not completed (.destroyForcibly process))
    (check completed "The child JVM did not complete within the bounded timeout.")
    (let [exit (.exitValue process)
          record {:mode mode :pid (.pid process) :parentPid (.pid (ProcessHandle/current))
                  :completed completed :exitStatus exit :command command}]
      (save-json (io/file directory "completion.json") record)
      (check (zero? exit) "The child JVM returned a failure status.")
      record)))

(defn validate-output [directory expected-mode expected-pid expected-sources]
  (let [receipt-file (io/file directory "receipt.json")
        lcov-file (io/file directory "measured.lcov")]
    (check (.isFile receipt-file) "The process receipt is missing.")
    (check (.isFile lcov-file) "The process LCOV artifact is missing.")
    (let [receipt (json/read-str (slurp receipt-file) :key-fn keyword)
          data (lines (slurp lcov-file))]
      (check (= expected-mode (:mode receipt)) "The process mode does not match its receipt.")
      (check (= expected-pid (:pid receipt)) "The process identity does not match its completion record.")
      (check (= expected-sources (:sources receipt)) "The source hashes do not match the parent inventory.")
      (check (= (set (vals sources)) (set (keys (:records data)))) "An expected source has no executable records.")
      (doseq [namespace ['coverage-research.disconnected 'coverage-research.child]]
        (check (contains? (get-in data [:records (sources namespace)]) 4) "An expected executable body line is missing."))
      (if (= expected-mode "baseline")
        (check (every? zero? (mapcat vals (vals (:records data)))) "The baseline contains a measured hit.")
        (check (pos? (hit data (sources 'coverage-research.child) 4)) "The child-only original line has no positive record."))
      data)))

(defn rejected? [f]
  (try (f) false (catch Throwable _ true)))

(def expected-locations
  [[(sources 'lowkeylab.clojure-sample.core) 7]
   [(sources 'lowkeylab.clojure-sample.core-test) 6]
   [(sources 'coverage-research.production) 10]
   [(sources 'coverage-research.production) 11]
   [(sources 'coverage-research.helper) 4]
   [(sources 'coverage-research.cases-test) 7]
   [(sources 'coverage-research.cases-test) 8]])

(defn location-observations [data]
  (mapv (fn [[path line]]
          {:path path :line line :recordPresent (contains? (get-in data [:records path]) line)
           :hits (hit data path line) :passed (pos? (hit data path line))
           :actualRecords (get-in data [:records path])}) expected-locations))

(def original-def-walker (get-method instrument/do-wrap :def))

(defmethod instrument/do-wrap :def [f line [_ name :as form] env]
  (let [name' (if-let [body (:test (meta name))]
                (vary-meta name assoc :test (instrument/wrap f line body))
                name)]
    (original-def-walker f line (with-meta (cons (first form) (cons name' (nnext form))) (meta form)) env)))

(defn copy-existing-aot [classes]
  (let [manifest (atom [])
        prefixes (map #(str/replace (str %) #"[-.]" {"-" "_" "." "/"}) (keys sources))]
    (doseq [path (str/split (System/getProperty "java.class.path") #":")
            :when (str/ends-with? path ".jar")]
      (with-open [jar (JarFile. path)]
        (doseq [entry (enumeration-seq (.entries jar))
                :let [name (.getName entry)]
                :when (and (str/ends-with? name ".class")
                           (some #(or (= name (str % "__init.class")) (str/starts-with? name (str % "$"))) prefixes))]
          (let [file (io/file classes name)]
            (when-not (.exists file)
              (.mkdirs (.getParentFile file))
              (with-open [input (.getInputStream jar entry)] (io/copy input file))
              (swap! manifest conj {:jar path :entry name :sha256 (sha256-bytes (java.nio.file.Files/readAllBytes (.toPath file)))}))))))
    (save-json (io/file (.getParentFile classes) "existing-aot-inputs.json") @manifest)))

(defn metadata-baseline [directory]
  (let [class-directory (io/file (.getParentFile (io/file directory)) "compiler-process" "classes")
        raw (atom [])]
    (check (every? #(not (find-ns %)) (keys sources)) "A source namespace loaded before metadata discovery.")
    (doseq [file (filter #(str/ends-with? (str %) ".class") (file-seq class-directory))]
      (let [reader (ClassReader. (java.nio.file.Files/readAllBytes (.toPath file)))
            owner (.getClassName reader)
            namespace (some (fn [n]
                              (let [prefix (str/replace (str n) #"[-.]" {"-" "_" "." "/"})]
                                (when (or (= owner (str prefix "__init"))
                                          (str/starts-with? owner (str prefix "$"))) n))) (keys sources))]
        (when namespace
          (.accept reader
                   (proxy [ClassVisitor] [262144]
                     (visitMethod [access name descriptor signature exceptions]
                       (proxy [MethodVisitor] [262144]
                         (visitLineNumber [line label]
                           (swap! raw conj {:class owner :method name :line line :path (sources namespace)})))))
                   0))))
    (check (seq @raw) "Compiler metadata contains no source lines.")
    (save-json (io/file directory "compiler-lines.json") @raw)
    (spit (io/file directory "measured.lcov")
          (apply str (for [[path records] (sort-by key (group-by :path @raw))]
                       (str "SF:" path "\n"
                            (apply str (for [line (sort (set (map :line records)))] (str "DA:" line ",0\n")))
                            "end_of_record\n"))))
    (check (every? #(not (find-ns %)) (keys sources)) "Metadata discovery evaluated a source namespace.")
    (check (not (.exists (io/file directory "initialization-marker.txt"))) "Metadata discovery executed initialization.")
    (save-json (io/file directory "receipt.json")
               {:mode "baseline" :pid (.pid (ProcessHandle/current)) :namespaceLoaded false
                :sources (mapv (fn [[n path]] {:path path :sha256 (sha256 (slurp (io/resource (source/resource-path n))))}) sources)
                :initializationMarkerPresent false :rawRecordCount (count @raw)})
    (save-json (io/file directory "metadata-expectations.json")
               (mapv (fn [[path line]] {:path path :line line :present (boolean (some #(and (= path (:path %)) (= line (:line %))) @raw))})
                     (conj expected-locations [(sources 'coverage-research.disconnected) 4] [(sources 'coverage-research.child) 4])))))

(defn -main [mode directory]
  (.mkdirs (io/file directory))
  (case mode
    "compile" (let [classes (io/file directory "classes")]
                (.mkdirs classes)
                (binding [*compile-path* (str classes)] (doseq [n (keys sources)] (compile n)))
                (check (.exists (io/file directory "initialization-marker.txt")) "Compilation did not execute initialization.")
                (copy-existing-aot classes))
    "baseline" (metadata-baseline directory)
    (binding [coverage/*covered* (atom [])]
    (check (not (find-ns 'lowkeylab.clojure-sample.core)) "The maintained AOT namespace loaded before instrumentation.")
    (check (not (find-ns 'coverage-research.production)) "The research AOT namespace loaded before instrumentation.")
    (let [aot-resources ["lowkeylab/clojure_sample/core__init.class" "coverage_research/production__init.class"]
          _ (check (every? io/resource aot-resources) "An expected AOT class is absent from the runtime.")
          mapping (into {} (map (fn [[namespace path]] [(source/resource-path namespace) path]) sources))
          hashes (mapv (fn [[namespace path]]
                         {:path path :sha256 (sha256 (slurp (io/resource (source/resource-path namespace))))}) sources)]
      (preserve-inputs directory aot-resources)
      (doseq [[namespace path] sources]
        (let [file (io/file directory "sources" path)]
          (.mkdirs (.getParentFile file))
          (spit file (slurp (io/resource (source/resource-path namespace))))))
      (coverage/instrument-namespaces {} (keys sources))
      (let [initialization (snapshot directory "initialization" mapping)
            _ (reset-hits)
            baseline (snapshot directory "baseline" mapping)
            _ (check (every? #(zero? (:hits %)) baseline) "The baseline contains initialization hits.")
            result (case mode
                     "baseline" {:noMeasuredCalls true}
                     "child" {:childValue ((requiring-resolve 'coverage-research.child/execute))}
                     "parent" (test/run-tests 'lowkeylab.clojure-sample.core-test 'coverage-research.cases-test)
                     (throw (ex-info "The research mode is invalid." {:mode mode})))
            measured (snapshot directory "measured" mapping)
            data (lines (slurp (io/file directory "measured.lcov")))]
        (when (= mode "parent")
          (check (and (zero? (:fail result)) (zero? (:error result))) "A functional Clojure test failed.")
          (save-json (io/file directory "expected-locations.json") (location-observations data))
          (check (every? :passed (location-observations data)) "An executed assertion has no original-line record.")
          (check (contains? (get-in data [:records (sources 'coverage-research.disconnected)]) 4) "The disconnected body has no executable record.")
          (check (contains? (get-in data [:records (sources 'coverage-research.child)]) 4) "The child-only body has no executable record.")
          (check (zero? (hit data (sources 'coverage-research.disconnected) 4)) "The disconnected function executed.")
          (check (zero? (hit data (sources 'coverage-research.child) 4)) "The child-only function executed in the parent."))
        (when (= mode "child") (check (= 42 (:childValue result)) "The child result is wrong."))
        (save-json (io/file directory "receipt.json")
                   {:mode mode :pid (.pid (ProcessHandle/current)) :javaVersion (System/getProperty "java.version")
                    :clojureVersion (clojure-version) :aotClassesPresent aot-resources :sources hashes
                    :initializationMarker @(var-get (requiring-resolve 'coverage-research.production/initialization-marker))
                    :initializationPositiveForms (count (filter :covered initialization))
                    :baselinePositiveForms (count (filter :covered baseline))
                    :measuredPositiveForms (count (filter :covered measured)) :functionalResult result})
        (when (= mode "parent")
          (let [baseline-directory (io/file directory "baseline-process")
                child-directory (io/file directory "child-process")
                compiler-completion (run-child "compile" (io/file directory "compiler-process"))
                baseline-completion (run-child "baseline" baseline-directory)
                child-completion (run-child "child" child-directory)
                baseline-data (lines (slurp (io/file baseline-directory "measured.lcov")))
                child-data (validate-output child-directory "child" (:pid child-completion) hashes)
                missing-directory (io/file directory "missing-control")
                corrupt-directory (io/file directory "corrupt-control")]
            (.mkdirs missing-directory)
            (.mkdirs corrupt-directory)
            (spit (io/file missing-directory "receipt.json") (slurp (io/file child-directory "receipt.json")))
            (spit (io/file corrupt-directory "receipt.json") (slurp (io/file child-directory "receipt.json")))
            (spit (io/file corrupt-directory "measured.lcov") "corrupt\n")
            (check (rejected? #(validate-output missing-directory "child" (:pid child-completion) hashes)) "Missing child output passed collection validation.")
            (check (rejected? #(validate-output corrupt-directory "child" (:pid child-completion) hashes)) "Corrupt child output passed collection validation.")
            (save-json (io/file directory "capabilities.json")
                       {:orchestrationPassed true :parentFunctionalResult result :childValue 42 :compilerMetadataHasZeroHits true
                        :parentLocationValidatorPassed (every? :passed (location-observations data))
                        :missingChildRejected true :corruptChildRejected true
                        :parentPid (.pid (ProcessHandle/current)) :childPid (:pid child-completion)
                        :baselinePid (:pid baseline-completion) :canonicalSources (vec (vals sources))
                        :disconnectedBodyHits (hit data (sources 'coverage-research.disconnected) 4)
                        :parentChildOnlyHits (hit data (sources 'coverage-research.child) 4)
                        :childOnlyHits (hit child-data (sources 'coverage-research.child) 4)
                        :baselineMeasuredHits (reduce + (mapcat vals (vals (:records baseline-data))))}))))))
)

)
