package rest

import (
	"errors"
	"log/slog"
	"net/http"
	"sort"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

// Handler wires circle HTTP endpoints.
type Handler struct {
	svc *service.Service
}

// NewHandler constructs a circle REST handler.
func NewHandler(svc *service.Service) *Handler {
	return &Handler{svc: svc}
}

// RegisterRoutes registers circle routes on the provided router.
func (h *Handler) RegisterRoutes(r gin.IRoutes) {
	r.POST("/circles", h.createCircle)
	r.GET("/circles", h.listUserCircles)
	r.POST("/circles/:id/members", h.addMember)
	r.POST("/circles/:id/join", h.joinCircle)
	r.GET("/circles/:id", h.getCircle)
	r.DELETE("/circles/:id", h.deleteCircle)
}

type createCircleRequest struct {
	Name string `json:"name"`
}

type memberResponse struct {
	UserID   int32  `json:"user_id"`
	Username string `json:"username"`
	Clout    int    `json:"clout"`
}

type circleResponse struct {
	ID        int32            `json:"id"`
	Name      string           `json:"name"`
	CreatedAt time.Time        `json:"created_at"`
	Members   []memberResponse `json:"members"`
}

func (h *Handler) createCircle(c *gin.Context) {
	var req createCircleRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		slog.WarnContext(c.Request.Context(), "invalid create circle request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated circle creation attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	newCircle, err := h.svc.CreateCircle(c.Request.Context(), req.Name, userID)
	if err != nil {
		slog.WarnContext(c.Request.Context(), "failed to create circle", "user_id", userID, "name", req.Name, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// Fetch with usernames for response
	enriched, err := h.svc.GetCircleWithUsernames(c.Request.Context(), newCircle.ID)
	if err != nil {
		slog.ErrorContext(c.Request.Context(), "failed to fetch circle", "circle_id", newCircle.ID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "circle created successfully", "circle_id", newCircle.ID, "user_id", userID, "name", req.Name)
	c.JSON(http.StatusCreated, toEnrichedCircleResponse(enriched))
}

func (h *Handler) listUserCircles(c *gin.Context) {
	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated list circles attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	circles, err := h.svc.ListUserCirclesWithUsernames(c.Request.Context(), userID)
	if err != nil {
		slog.ErrorContext(c.Request.Context(), "failed to list user circles", "user_id", userID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	responses := make([]circleResponse, len(circles))
	for i, circ := range circles {
		responses[i] = toEnrichedCircleResponse(circ)
	}

	slog.DebugContext(c.Request.Context(), "circles listed", "user_id", userID, "count", len(circles))
	c.JSON(http.StatusOK, responses)
}

type addMemberRequest struct {
	UserID int32 `json:"user_id"`
}

func (h *Handler) addMember(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	var req addMemberRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		slog.WarnContext(c.Request.Context(), "invalid add member request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.svc.AddMember(c.Request.Context(), circleID, user.ID(req.UserID))
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		slog.ErrorContext(c.Request.Context(), "failed to add member", "circle_id", circleID, "user_id", req.UserID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "member added to circle", "circle_id", circleID, "user_id", req.UserID)
	c.Status(http.StatusCreated)
}

func (h *Handler) joinCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated join circle attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	err := h.svc.JoinCircle(c.Request.Context(), circleID, userID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		slog.WarnContext(c.Request.Context(), "failed to join circle", "circle_id", circleID, "user_id", userID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "user joined circle", "circle_id", circleID, "user_id", userID)
	c.Status(http.StatusCreated)
}

func (h *Handler) getCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	result, err := h.svc.GetCircleWithUsernames(c.Request.Context(), circleID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.DebugContext(c.Request.Context(), "circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		slog.ErrorContext(c.Request.Context(), "failed to get circle", "circle_id", circleID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	slog.DebugContext(c.Request.Context(), "circle retrieved", "circle_id", circleID)
	c.JSON(http.StatusOK, toEnrichedCircleResponse(result))
}

func (h *Handler) deleteCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		slog.WarnContext(c.Request.Context(), "unauthenticated delete circle attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	err := h.svc.DeleteCircle(c.Request.Context(), circleID, userID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			slog.WarnContext(c.Request.Context(), "circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		if errors.Is(err, service.ErrNotCircleOwner) {
			slog.WarnContext(c.Request.Context(), "unauthorized circle deletion", "circle_id", circleID, "user_id", userID, "error", err)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		slog.ErrorContext(c.Request.Context(), "failed to delete circle", "circle_id", circleID, "user_id", userID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	slog.InfoContext(c.Request.Context(), "circle deleted", "circle_id", circleID, "user_id", userID)
	c.Status(http.StatusNoContent)
}

func parseCircleID(c *gin.Context) (circle.ID, bool) {
	rawID := c.Param("id")
	if rawID == "" {
		slog.WarnContext(c.Request.Context(), "missing circle id parameter")
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing circle id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		slog.WarnContext(c.Request.Context(), "invalid circle id format", "id", rawID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid circle id"})
		return 0, false
	}

	return circle.ID(parsed), true
}

func toEnrichedCircleResponse(enriched *service.EnrichedCircle) circleResponse {
	members := make([]memberResponse, len(enriched.Members))
	for i, m := range enriched.Members {
		members[i] = memberResponse{
			UserID:   int32(m.UserID),
			Username: m.Username,
			Clout:    m.Clout,
		}
	}

	sort.Slice(members, func(i, j int) bool {
		return members[i].UserID < members[j].UserID
	})

	return circleResponse{
		ID:        int32(enriched.ID),
		Name:      enriched.Name,
		CreatedAt: enriched.CreatedAt,
		Members:   members,
	}
}
