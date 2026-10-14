package rust_language

import (
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"os/exec"
	"time"

	"github.com/bazelbuild/rules_go/go/tools/bazel"

	"google.golang.org/protobuf/encoding/protowire"
	"google.golang.org/protobuf/proto"

	pb "github.com/calsign/gazelle_rust/proto"
)

type Parser struct {
	cmd            *exec.Cmd
	stdin          io.WriteCloser
	stdout         io.ReadCloser
	marshalOptions *proto.MarshalOptions
	responseBuffer []byte
	cleanupDone    bool
	cleanupErr     error
}

func NewParser() *Parser {
	path, err := bazel.Runfile("3rdparty/gazelle_rust/upstream/rust_parser/rust_parser")
	if err != nil {
		log.Fatal(err)
	}
	p, err := startParser(exec.Command(path, "stream-proto"))
	if err != nil {
		log.Fatal(err)
	}
	return p
}

// startParser owns the command and all pipes it acquires, including on failure.
func startParser(cmd *exec.Cmd) (*Parser, error) {
	p := &Parser{cmd: cmd, marshalOptions: &proto.MarshalOptions{}, responseBuffer: make([]byte, 1024)}
	// Keep both ends explicit so a later pipe-creation or launch failure can
	// release every descriptor, even before exec.Cmd has a process to wait for.
	stdinRead, stdinWrite, err := os.Pipe()
	if err != nil {
		return nil, fmt.Errorf("parser stdin: %w", err)
	}
	defer stdinRead.Close()
	p.stdin = stdinWrite
	stdoutRead, stdoutWrite, err := os.Pipe()
	if err != nil {
		return nil, errors.Join(fmt.Errorf("parser stdout: %w", err), p.cleanup())
	}
	defer stdoutWrite.Close()
	p.stdout = stdoutRead
	cmd.Stdin, cmd.Stdout = stdinRead, stdoutWrite
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		return nil, errors.Join(fmt.Errorf("start parser %q: %w", cmd.Path, err), p.cleanup())
	}
	return p, nil
}

// cleanup ends this sequential session. EOF gives the child a chance to exit
// normally; only shutdown has a deadline, never an individual parse request.
func (p *Parser) cleanup() error {
	if p.cleanupDone {
		return p.cleanupErr
	}
	p.cleanupDone = true
	if p.stdin != nil {
		if err := p.stdin.Close(); err != nil && !errors.Is(err, os.ErrClosed) {
			p.cleanupErr = fmt.Errorf("close parser stdin: %w", err)
		}
	}
	if p.cmd != nil && p.cmd.Process != nil {
		exited := make(chan error, 1)
		go func() { exited <- p.cmd.Wait() }()
		timer := time.NewTimer(5 * time.Second)
		defer timer.Stop()
		select {
		case err := <-exited:
			if err != nil {
				p.cleanupErr = errors.Join(p.cleanupErr, fmt.Errorf("wait for parser: %w", err))
			}
		case <-timer.C:
			if err := p.cmd.Process.Kill(); err != nil && !errors.Is(err, os.ErrProcessDone) {
				p.cleanupErr = errors.Join(p.cleanupErr, fmt.Errorf("kill parser: %w", err))
			}
			// Wait exactly once, including after a forced termination.
			if err := <-exited; err != nil {
				p.cleanupErr = errors.Join(p.cleanupErr, fmt.Errorf("wait for parser after shutdown timeout: %w", err))
			}
		}
	}
	if p.stdout != nil {
		if err := p.stdout.Close(); err != nil && !errors.Is(err, os.ErrClosed) {
			p.cleanupErr = errors.Join(p.cleanupErr, fmt.Errorf("close parser stdout: %w", err))
		}
	}
	return p.cleanupErr
}

func (p *Parser) writeRequest(request *pb.Request) error {
	size := p.marshalOptions.Size(request)

	buf := []byte{}
	buf = protowire.AppendFixed32(buf, uint32(size))
	buf, err := p.marshalOptions.MarshalAppend(buf, request)
	if err != nil {
		return err
	}

	for len(buf) > 0 {
		n, err := p.stdin.Write(buf)
		if err != nil {
			return fmt.Errorf("write parser request: %w", err)
		}
		if n <= 0 || n > len(buf) {
			return fmt.Errorf("write parser request: %w", io.ErrShortWrite)
		}
		buf = buf[n:]
	}
	return nil
}

func (p *Parser) readResponse(response proto.Message) error {
	sf32 := protowire.SizeFixed32()
	n, err := io.ReadFull(p.stdout, p.responseBuffer[:sf32])
	if err != nil {
		return fmt.Errorf("read parser response prefix (%d of %d bytes): %w", n, sf32, err)
	}
	size, _ := protowire.ConsumeFixed32(p.responseBuffer[:sf32])
	if size > uint32(len(p.responseBuffer)) {
		// Grow the response buffer as needed.
		p.responseBuffer = make([]byte, size)
	}
	n, err = io.ReadFull(p.stdout, p.responseBuffer[:size])
	if err != nil {
		return fmt.Errorf("read parser response body (%d of %d bytes): %w", n, size, err)
	}
	if err := proto.Unmarshal(p.responseBuffer[:size], response); err != nil {
		return fmt.Errorf("decode parser response: %w", err)
	}

	return nil
}

func (p *Parser) exchange(request *pb.Request, response proto.Message) error {
	if p.cleanupDone {
		return errors.New("parser session is closed")
	}
	err := p.writeRequest(request)
	if err == nil {
		err = p.readResponse(response)
	}
	if err != nil {
		return errors.Join(err, p.cleanup())
	}
	return nil
}

func (p *Parser) Parse(request *pb.RustImportsRequest) (*pb.RustImportsResponse, error) {
	response := &pb.RustImportsResponse{}
	if err := p.exchange(&pb.Request{
		Kind: &pb.Request_RustImports{RustImports: request},
	}, response); err != nil {
		return nil, err
	}
	return response, nil
}

func (p *Parser) GetLockfileCrates(request *pb.LockfileCratesRequest) (*pb.LockfileCratesResponse, error) {
	response := &pb.LockfileCratesResponse{}
	if err := p.exchange(&pb.Request{
		Kind: &pb.Request_LockfileCrates{LockfileCrates: request},
	}, response); err != nil {
		return nil, err
	}
	return response, nil
}

func (p *Parser) ParseCargoToml(request *pb.CargoTomlRequest) (*pb.CargoTomlResponse, error) {
	response := &pb.CargoTomlResponse{}
	if err := p.exchange(&pb.Request{
		Kind: &pb.Request_CargoToml{CargoToml: request},
	}, response); err != nil {
		return nil, err
	}
	return response, nil
}
