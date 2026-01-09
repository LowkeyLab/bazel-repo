package service_test

import (
	authorizer "github.com/authorizerdev/authorizer-go"
	"github.com/stretchr/testify/mock"
)

type MockAuthorizerClient struct {
	mock.Mock
}

func (m *MockAuthorizerClient) GetProfile(headers map[string]string) (*authorizer.User, error) {
	args := m.Called(headers)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).(*authorizer.User), args.Error(1)
}
