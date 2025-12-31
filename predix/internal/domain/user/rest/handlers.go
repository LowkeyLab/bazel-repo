package rest

import (
	"net/http"

	"github.com/gin-gonic/gin"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user/service"
)

// Handler wires user authentication endpoints.
type Handler struct {
	svc    *service.Service
	tokens *auth.Manager
}

// NewHandler constructs a user REST handler.
func NewHandler(svc *service.Service, tokens *auth.Manager) *Handler {
	return &Handler{svc: svc, tokens: tokens}
}

// RegisterRoutes registers user-related routes on the provided router.
func (h *Handler) RegisterRoutes(r gin.IRoutes) {
	r.POST("/login", h.login)
}

type loginRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

type userResponse struct {
	ID       int32  `json:"id"`
	Username string `json:"username"`
}

type loginResponse struct {
	Token string       `json:"token"`
	User  userResponse `json:"user"`
}

func (h *Handler) login(c *gin.Context) {
	var req loginRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	u, err := h.svc.Login(c.Request.Context(), req.Username, req.Password)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	token, err := h.tokens.GenerateToken(u)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to issue token"})
		return
	}

	c.JSON(http.StatusOK, loginResponse{
		Token: token,
		User:  toUserResponse(u),
	})
}

func toUserResponse(u *user.User) userResponse {
	return userResponse{
		ID:       int32(u.ID),
		Username: u.Username,
	}
}
