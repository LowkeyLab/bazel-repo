package rest

import (
	"errors"
	"net/http"
	"sort"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
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
func (h *Handler) RegisterRoutes(r *gin.Engine) {
	r.POST("/circles", h.createCircle)
	r.POST("/circles/:id/members", h.addMember)
	r.GET("/circles/:id", h.getCircle)
}

type createCircleRequest struct {
	Name      string `json:"name"`
	CreatorID int32  `json:"creator_id"`
}

type memberResponse struct {
	UserID int32 `json:"user_id"`
	Clout  int   `json:"clout"`
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
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	newCircle, err := h.svc.CreateCircle(c.Request.Context(), req.Name, user.ID(req.CreatorID))
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusCreated, toCircleResponse(newCircle))
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
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.svc.AddMember(c.Request.Context(), circleID, user.ID(req.UserID))
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.Status(http.StatusCreated)
}

func (h *Handler) getCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	result, err := h.svc.GetCircle(c.Request.Context(), circleID)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, toCircleResponse(result))
}

func parseCircleID(c *gin.Context) (circle.ID, bool) {
	rawID := c.Param("id")
	if rawID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing circle id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid circle id"})
		return 0, false
	}

	return circle.ID(parsed), true
}

func toCircleResponse(circ *circle.Circle) circleResponse {
	members := make([]memberResponse, 0, len(circ.Members))
	for _, m := range circ.Members {
		members = append(members, memberResponse{
			UserID: int32(m.UserID),
			Clout:  m.Clout,
		})
	}

	sort.Slice(members, func(i, j int) bool {
		return members[i].UserID < members[j].UserID
	})

	return circleResponse{
		ID:        int32(circ.ID),
		Name:      circ.Name,
		CreatedAt: circ.CreatedAt,
		Members:   members,
	}
}
