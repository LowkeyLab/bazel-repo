package rust_language

import (
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"os/exec"

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
}

func NewParser() *Parser {
	path, err := bazel.Runfile("3rdparty/gazelle_rust/upstream/rust_parser/rust_parser")
	if err != nil {
		log.Fatal(err)
	}
	cmd := exec.Command(path, "stream-proto")
	stdin, err := cmd.StdinPipe()
	if err != nil {
		log.Fatal(err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		log.Fatal(err)
	}
	cmd.Stderr = os.Stderr
	cmd.Start()
	return &Parser{
		cmd:            cmd,
		stdin:          stdin,
		stdout:         stdout,
		marshalOptions: &proto.MarshalOptions{},
		responseBuffer: make([]byte, 1024),
	}
}

func (p *Parser) writeRequest(request *pb.Request) error {
	size := p.marshalOptions.Size(request)

	buf := []byte{}
	buf = protowire.AppendFixed32(buf, uint32(size))
	buf, err := p.marshalOptions.MarshalAppend(buf, request)
	if err != nil {
		return err
	}

	p.stdin.Write(buf)

	return nil
}

func (p *Parser) readResponse(response proto.Message) error {
	sf32 := protowire.SizeFixed32()
	n, err := io.ReadFull(p.stdout, p.responseBuffer[:sf32])
	if n != sf32 {
		return errors.New("invalid size")
	}
	if err != nil {
		return err
	}
	size, n := protowire.ConsumeFixed32(p.responseBuffer[:sf32])
	if n != sf32 {
		log.Fatalf("n: %v\n", n)
	}
	if size > uint32(len(p.responseBuffer)) {
		// Grow the response buffer as needed.
		p.responseBuffer = make([]byte, size)
	}
	n, err = io.ReadFull(p.stdout, p.responseBuffer[:size])
	if err != nil {
		return err
	}
	if uint32(n) != size {
		return errors.New(fmt.Sprintf(
			"recieved wrong size %d, expected %d", n, size))
	}
	err = proto.Unmarshal(p.responseBuffer[:size], response)
	if err != nil {
		return err
	}

	return nil
}

func (p *Parser) exchange(request *pb.Request, response proto.Message) error {
	if err := p.writeRequest(request); err != nil {
		return err
	}
	return p.readResponse(response)
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
