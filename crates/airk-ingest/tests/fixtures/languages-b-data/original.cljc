(ns archive.core (:require [clojure.string :as str]))
(defn render [value] (str/upper-case value))
#?(:clj (def platform :jvm) :cljs (def platform :js))
(comment (defn phantom [] nil))
