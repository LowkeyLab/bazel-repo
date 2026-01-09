package auth

import (
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// TestUserIDHeader is the header tests can set to inject a user ID.
const TestUserIDHeader = "X-Test-User-ID"

// TestMiddleware injects a user ID from a test header without validating tokens.
func TestMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		id := strings.TrimSpace(c.GetHeader(TestUserIDHeader))
		if id == "" {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "missing test user id"})
			return
		}

		c.Set(userIDContextKey, user.ID(id))
		c.Set(userNameContextKey, "test-"+id)
		c.Next()
	}
}
