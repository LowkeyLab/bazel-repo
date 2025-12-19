package main

import (
	"net/http"

	"nmyk.io/cowsay"
)

func main() {
	http.HandleFunc("/", HelloServer)
	http.ListenAndServe(":8080", nil)
}

func HelloServer(w http.ResponseWriter, r *http.Request) {
	cowsay.Cow{}.Write(w, []byte("Hello, world!"), false)
}
