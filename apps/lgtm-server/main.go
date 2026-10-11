package main

import (
	"flag"
	"github.com/difflab-io/lgtm/internal/hosting"
	"log"
	"os"
	"os/signal"
	"syscall"
)

func main() {
	options := hosting.BindFlags(flag.CommandLine)
	flag.Parse()
	stop, err := hosting.StartConfigured(options.Config())
	if err != nil {
		log.Fatal(err)
	}
	defer stop.Close()
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
	<-signals
}
