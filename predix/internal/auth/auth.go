package auth

import (
	"net/http"
	"strings"

	authorizer "github.com/authorizerdev/authorizer-go"
	"github.com/gin-gonic/gin"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

const (
	userIDContextKey   = "currentUserID"
	userNameContextKey = "currentUsername"
)

// Client defines the interface for interacting with the Authorizer service.
// This interface allows for mocking the Authorizer client in tests.
type Client interface {
	GetProfile(headers map[string]string) (*authorizer.User, error)
}

// Manager validates authentication tokens using Authorizer.
type Manager interface {
	Middleware() gin.HandlerFunc
}

type authorizerManager struct {
	client Client
}

// NewManager constructs a Manager with the provided Authorizer client.
func NewManager(client Client) Manager {
	return &authorizerManager{client: client}
}

// Middleware enforces authentication and injects the user ID and name into the context.
func (m *authorizerManager) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		authHeader := c.GetHeader("Authorization")
		if authHeader == "" {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "authorization required"})
			return
		}

		parts := strings.SplitN(authHeader, " ", 2)
		if len(parts) != 2 || !strings.EqualFold(parts[0], "bearer") {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "invalid authorization header"})
			return
		}

		// Use GetProfile to validate the token/session and get user details
		resUser, err := m.client.GetProfile(map[string]string{"Authorization": authHeader})
		if err != nil {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "invalid token: " + err.Error()})
			return
		}

		if resUser == nil {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "user not found in session"})
			return
		}

		// Use nickname if available, else email, else ID as username
		username := resUser.Nickname
		if username == nil || *username == "" {
			username = &resUser.Email
		}
		if *username == "" {
			username = &resUser.ID
		}

		c.Set(userIDContextKey, user.ID(resUser.ID))
		c.Set(userNameContextKey, *username)
		c.Next()
	}
}

// UserIDFromContext extracts the authenticated user ID from the Gin context.
func UserIDFromContext(c *gin.Context) (user.ID, bool) {
	value, ok := c.Get(userIDContextKey)
	if !ok {
		return "", false
	}

	id, ok := value.(user.ID)
	return id, ok
}

// UserNameFromContext extracts the authenticated username from the Gin context.
func UserNameFromContext(c *gin.Context) (string, bool) {
	value, ok := c.Get(userNameContextKey)
	if !ok {
		return "", false
	}

	name, ok := value.(string)
	return name, ok
}
