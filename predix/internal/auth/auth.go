package auth

import (
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/golang-jwt/jwt/v5"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

const userIDContextKey = "currentUserID"

// Manager issues and validates authentication tokens.
type Manager struct {
	secret []byte
	ttl    time.Duration
}

// NewManager constructs a Manager with the provided secret and token TTL.
func NewManager(secret string, ttl time.Duration) *Manager {
	if secret == "" {
		secret = "dev-secret"
	}
	if ttl <= 0 {
		ttl = 24 * time.Hour
	}

	return &Manager{secret: []byte(secret), ttl: ttl}
}

// GenerateToken signs a JWT for the given user.
func (m *Manager) GenerateToken(u *user.User) (string, error) {
	if u == nil {
		return "", errors.New("user is required")
	}
	if u.ID == 0 {
		return "", errors.New("user id is required")
	}

	claims := jwt.RegisteredClaims{
		ExpiresAt: jwt.NewNumericDate(time.Now().Add(m.ttl)),
		IssuedAt:  jwt.NewNumericDate(time.Now()),
	}

	token := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{
		"uid": int32(u.ID),
		"exp": claims.ExpiresAt.Unix(),
		"iat": claims.IssuedAt.Unix(),
	})

	return token.SignedString(m.secret)
}

// ParseToken validates a JWT and extracts the user ID.
func (m *Manager) ParseToken(raw string) (user.ID, error) {
	parsed, err := jwt.Parse(raw, func(t *jwt.Token) (interface{}, error) {
		if _, ok := t.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", t.Header["alg"])
		}
		return m.secret, nil
	})
	if err != nil {
		return 0, fmt.Errorf("invalid token: %w", err)
	}

	claims, ok := parsed.Claims.(jwt.MapClaims)
	if !ok || !parsed.Valid {
		return 0, errors.New("invalid token claims")
	}

	uid, ok := claims["uid"].(float64)
	if !ok {
		return 0, errors.New("user id missing from token")
	}

	return user.ID(int32(uid)), nil
}

// Middleware enforces authentication and injects the user ID into the context.
func (m *Manager) Middleware() gin.HandlerFunc {
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

		userID, err := m.ParseToken(parts[1])
		if err != nil {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "invalid token"})
			return
		}

		c.Set(userIDContextKey, userID)
		c.Next()
	}
}

// UserIDFromContext extracts the authenticated user ID from the Gin context.
func UserIDFromContext(c *gin.Context) (user.ID, bool) {
	value, ok := c.Get(userIDContextKey)
	if !ok {
		return 0, false
	}

	id, ok := value.(user.ID)
	return id, ok
}
