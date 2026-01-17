package rest

import (
	"database/sql"
	"errors"
	"io"
	"net/http"
	"sort"
	"strconv"
	"time"

	sloggin "github.com/gin-contrib/slog"
	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"github.com/lowkeylab/bazel-repo/predix/internal/auth"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/service"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/circle/sse"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/contest"
	"github.com/lowkeylab/bazel-repo/predix/internal/domain/user"
)

type EventSubscriber interface {
	Subscribe(contestID contest.ID) (chan sse.Event, func())
}

// Handler wires circle HTTP endpoints.
type Handler struct {
	svc        *service.Service
	subscriber EventSubscriber
}

// NewHandler constructs a circle REST handler.
func NewHandler(svc *service.Service, subscriber EventSubscriber) *Handler {
	return &Handler{svc: svc, subscriber: subscriber}
}

// RegisterRoutes registers circle routes on the provided router.
func (h *Handler) RegisterRoutes(r gin.IRoutes) {
	r.POST("/circles", h.createCircle)
	r.GET("/circles", h.listUserCircles)
	r.POST("/circles/:id/members", h.addMember)
	r.POST("/circles/:id/join", h.joinCircle)
	r.GET("/circles/:id", h.getCircle)
	r.GET("/circles/:id/contests", h.getCircleContests)
	r.POST("/circles/:id/contests", h.createContest)
	r.GET("/circles/:id/contests/:contestID", h.getContest)
	r.GET("/circles/:id/contests/:contestID/stream", sseHeadersMiddleware(), h.streamContestEvents)
	r.POST("/circles/:id/contests/:contestID/predictions", h.makePrediction)
	r.POST("/circles/:id/contests/:contestID/lock", h.lockContest)
	r.POST("/circles/:id/contests/:contestID/resolve-distribute", h.resolveAndDistribute)
	r.GET("/circles/:id/contests/:contestID/payout-breakdown", h.getPayoutBreakdown)
	r.DELETE("/circles/:id", h.deleteCircle)
}

type createCircleRequest struct {
	Name string `json:"name"`
}

type memberResponse struct {
	UserID   string `json:"user_id"`
	Username string `json:"username"`
	Clout    int    `json:"clout"`
}

type circleResponse struct {
	ID        int32            `json:"id"`
	Name      string           `json:"name"`
	CreatedAt time.Time        `json:"created_at"`
	Members   []memberResponse `json:"members"`
}

type optionResponse struct {
	ID   int    `json:"id"`
	Text string `json:"text"`
}

type predictionResponse struct {
	UserID    string    `json:"user_id"`
	OptionID  int       `json:"option_id"`
	Clout     int       `json:"clout"`
	Timestamp time.Time `json:"timestamp"`
}

type contestResponse struct {
	ID             int32                `json:"id"`
	CircleID       int32                `json:"circle_id"`
	CreatorID      string               `json:"creator_id"`
	Question       string               `json:"question"`
	Options        []optionResponse     `json:"options"`
	Predictions    []predictionResponse `json:"predictions"`
	Status         string               `json:"status"`
	MinStake       int                  `json:"min_stake"`
	TotalPot       int                  `json:"total_pot"`
	HouseRake      int                  `json:"house_rake"`
	ResultOptionID *int                 `json:"result_option_id,omitempty"`
	CreatedAt      time.Time            `json:"created_at"`
	LockedAt       time.Time            `json:"locked_at"`
	Duration       string               `json:"duration"`
}

type addMemberRequest struct {
	UserID string `json:"user_id"`
}

type makePredictionRequest struct {
	OptionID int `json:"option_id"`
	Clout    int `json:"clout"`
}

type resolveContestRequest struct {
	WinningOptionID int `json:"winning_option_id"`
}

type createContestRequest struct {
	Question string   `json:"question"`
	Options  []string `json:"options"`
	MinStake int      `json:"min_stake"`
	Duration string   `json:"expiration_duration"`
}

type payoutRecord struct {
	UserID   string `json:"user_id"`
	Username string `json:"username"`
	Stake    int    `json:"stake"`
	Share    int    `json:"share"`
	Total    int    `json:"total"`
}

type payoutBreakdownResponse struct {
	Winners          []payoutRecord `json:"winners"`
	Losers           []payoutRecord `json:"losers"`
	TotalPot         int            `json:"total_pot"`
	HouseRake        int            `json:"house_rake"`
	DistributablePot int            `json:"distributable_pot"`
	TotalDistributed int            `json:"total_distributed"`
}

func (h *Handler) createCircle(c *gin.Context) {
	var req createCircleRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid create circle request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated circle creation attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	newCircle, err := h.svc.CreateCircle(c.Request.Context(), req.Name, userID)
	if err != nil {
		sloggin.Get(c).Warn("failed to create circle", "user_id", userID, "name", req.Name, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	enriched, err := h.svc.GetCircleWithUsernames(c.Request.Context(), newCircle.ID)
	if err != nil {
		sloggin.Get(c).Error("failed to fetch circle", "circle_id", newCircle.ID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("circle created successfully", "circle_id", newCircle.ID, "user_id", userID, "name", req.Name)
	c.JSON(http.StatusCreated, toEnrichedCircleResponse(enriched))
}

func (h *Handler) listUserCircles(c *gin.Context) {
	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated list circles attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	circles, err := h.svc.ListUserCirclesWithUsernames(c.Request.Context(), userID)
	if err != nil {
		sloggin.Get(c).Error("failed to list user circles", "user_id", userID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	responses := make([]circleResponse, len(circles))
	for i, circ := range circles {
		responses[i] = toEnrichedCircleResponse(circ)
	}

	sloggin.Get(c).Debug("circles listed", "user_id", userID, "count", len(circles))
	c.JSON(http.StatusOK, responses)
}

func (h *Handler) addMember(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	var req addMemberRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid add member request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	userID, err := uuid.Parse(req.UserID)
	if err != nil {
		sloggin.Get(c).Warn("invalid user id in request", "user_id", req.UserID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid user id"})
		return
	}

	err = h.svc.AddMember(c.Request.Context(), circleID, user.ID(userID))
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			sloggin.Get(c).Warn("circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		sloggin.Get(c).Error("failed to add member", "circle_id", circleID, "user_id", req.UserID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("member added to circle", "circle_id", circleID, "user_id", req.UserID)
	c.Status(http.StatusCreated)
}

func (h *Handler) joinCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated join circle attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	err := h.svc.JoinCircle(c.Request.Context(), circleID, userID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			sloggin.Get(c).Warn("circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		sloggin.Get(c).Warn("failed to join circle", "circle_id", circleID, "user_id", userID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("user joined circle", "circle_id", circleID, "user_id", userID)
	c.Status(http.StatusCreated)
}

func (h *Handler) getCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	result, err := h.svc.GetCircleWithUsernames(c.Request.Context(), circleID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			sloggin.Get(c).Debug("circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		sloggin.Get(c).Error("failed to get circle", "circle_id", circleID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Debug("circle retrieved", "circle_id", circleID)
	c.JSON(http.StatusOK, toEnrichedCircleResponse(result))
}

func (h *Handler) getCircleContests(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contests, err := h.svc.GetContestsByCircleID(c.Request.Context(), circleID)
	if err != nil {
		sloggin.Get(c).Error("failed to get circle contests", "circle_id", circleID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	responses := make([]contestResponse, len(contests))
	for i, cont := range contests {
		responses[i] = toContestResponse(cont)
	}

	sloggin.Get(c).Debug("circle contests retrieved", "circle_id", circleID, "count", len(contests))
	c.JSON(http.StatusOK, responses)
}

func (h *Handler) deleteCircle(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated delete circle attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	err := h.svc.DeleteCircle(c.Request.Context(), circleID, userID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			sloggin.Get(c).Warn("circle not found", "circle_id", circleID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle not found"})
			return
		}
		if errors.Is(err, service.ErrNotCircleOwner) {
			sloggin.Get(c).Warn("unauthorized circle deletion", "circle_id", circleID, "user_id", userID, "error", err)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		sloggin.Get(c).Error("failed to delete circle", "circle_id", circleID, "user_id", userID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("circle deleted", "circle_id", circleID, "user_id", userID)
	c.Status(http.StatusNoContent)
}

func (h *Handler) makePrediction(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated prediction attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	var req makePredictionRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid make prediction request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.svc.Predict(c.Request.Context(), circleID, contestID, userID, req.OptionID, req.Clout)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || errors.Is(err, service.ErrContestNotFound) {
			sloggin.Get(c).Warn("circle or contest not found", "circle_id", circleID, "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle or contest not found"})
			return
		}
		if errors.Is(err, service.ErrUserNotInCircle) {
			sloggin.Get(c).Warn("user not in circle", "circle_id", circleID, "user_id", userID)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		if errors.Is(err, service.ErrInsufficientClout) {
			sloggin.Get(c).Warn("insufficient clout", "circle_id", circleID, "user_id", userID)
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		sloggin.Get(c).Warn("failed to make prediction", "circle_id", circleID, "contest_id", contestID, "user_id", userID, "option_id", req.OptionID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("prediction made successfully", "circle_id", circleID, "contest_id", contestID, "user_id", userID, "option_id", req.OptionID)
	c.Status(http.StatusCreated)
}

func (h *Handler) resolveAndDistribute(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contestID, ok := parseContestID(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated resolve attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	var req resolveContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid resolve contest request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	err := h.svc.ResolveAndDistributeContestClout(c.Request.Context(), circleID, contestID, userID, req.WinningOptionID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || errors.Is(err, service.ErrContestNotFound) {
			sloggin.Get(c).Warn("circle or contest not found", "circle_id", circleID, "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "circle or contest not found"})
			return
		}
		if errors.Is(err, service.ErrNotContestCreator) {
			sloggin.Get(c).Warn("unauthorized contest resolution", "contest_id", contestID, "user_id", userID)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		sloggin.Get(c).Warn("failed to resolve contest", "circle_id", circleID, "contest_id", contestID, "user_id", userID, "winning_option_id", req.WinningOptionID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("contest resolved and clout distributed", "circle_id", circleID, "contest_id", contestID, "user_id", userID, "winning_option_id", req.WinningOptionID)
	c.Status(http.StatusOK)
}

func (h *Handler) createContest(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	var req createContestRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		sloggin.Get(c).Warn("invalid create contest request body", "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid request body"})
		return
	}

	creatorID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated contest creation attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	// Validate duration
	validDurations := contest.ValidDurations()
	isValid := false
	for _, d := range validDurations {
		if req.Duration == d {
			isValid = true
			break
		}
	}
	if !isValid {
		sloggin.Get(c).Warn("invalid duration provided", "duration", req.Duration)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid duration: must be one of: 1h, 1d, 1w"})
		return
	}

	newContest, err := h.svc.CreateContest(
		c.Request.Context(),
		circleID,
		creatorID,
		req.Question,
		req.Options,
		req.Duration,
		req.MinStake,
	)
	if err != nil {
		sloggin.Get(c).Warn("failed to create contest", "creator_id", creatorID, "question", req.Question, "circle_id", circleID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("contest created successfully", "contest_id", newContest.ID, "creator_id", creatorID, "question", req.Question, "circle_id", circleID)
	c.JSON(http.StatusCreated, toContestResponse(newContest))
}

func (h *Handler) getContest(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contestID, ok := parseContestIDFromRoute(c)
	if !ok {
		return
	}

	result, err := h.svc.GetContestInCircle(c.Request.Context(), circleID, contestID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || errors.Is(err, service.ErrContestNotFound) {
			sloggin.Get(c).Debug("contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		sloggin.Get(c).Error("failed to get contest", "contest_id", contestID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Debug("contest retrieved", "contest_id", contestID)
	c.JSON(http.StatusOK, toContestResponse(result))
}

func (h *Handler) lockContest(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contestID, ok := parseContestIDFromRoute(c)
	if !ok {
		return
	}

	userID, ok := auth.UserIDFromContext(c)
	if !ok {
		sloggin.Get(c).Warn("unauthenticated lock attempt")
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication required"})
		return
	}

	err := h.svc.LockContest(c.Request.Context(), circleID, contestID, userID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || errors.Is(err, service.ErrContestNotFound) {
			sloggin.Get(c).Warn("contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		if errors.Is(err, service.ErrNotContestCreator) {
			sloggin.Get(c).Warn("unauthorized contest lock", "contest_id", contestID, "user_id", userID)
			c.JSON(http.StatusForbidden, gin.H{"error": err.Error()})
			return
		}
		sloggin.Get(c).Warn("failed to lock contest", "contest_id", contestID, "user_id", userID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	sloggin.Get(c).Info("contest locked successfully", "contest_id", contestID, "user_id", userID)
	c.Status(http.StatusOK)
}

func (h *Handler) getPayoutBreakdown(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contestID, ok := parseContestIDFromRoute(c)
	if !ok {
		return
	}

	breakdown, err := h.svc.GetPayoutBreakdown(c.Request.Context(), circleID, contestID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || errors.Is(err, service.ErrContestNotFound) {
			sloggin.Get(c).Warn("contest not found", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		// Assuming GetPayoutBreakdown validates status and returns specific error if not resolved
		sloggin.Get(c).Warn("failed to calculate payout breakdown", "contest_id", contestID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	winners := make([]payoutRecord, len(breakdown.Winners))
	for i, r := range breakdown.Winners {
		winners[i] = payoutRecord{
			UserID:   uuid.UUID(r.UserID).String(),
			Username: r.Username,
			Stake:    r.Stake,
			Share:    r.Share,
			Total:    r.Total,
		}
	}

	losers := make([]payoutRecord, len(breakdown.Losers))
	for i, r := range breakdown.Losers {
		losers[i] = payoutRecord{
			UserID:   uuid.UUID(r.UserID).String(),
			Username: r.Username,
			Stake:    r.Stake,
			Share:    r.Share,
			Total:    r.Total,
		}
	}

	response := payoutBreakdownResponse{
		Winners:          winners,
		Losers:           losers,
		TotalPot:         breakdown.TotalPot,
		HouseRake:        breakdown.HouseRake,
		DistributablePot: breakdown.DistributablePot,
		TotalDistributed: breakdown.TotalDistributed,
	}

	sloggin.Get(c).Info("payout breakdown retrieved", "contest_id", contestID, "winner_count", len(breakdown.Winners))
	c.JSON(http.StatusOK, response)
}

func parseCircleID(c *gin.Context) (circle.ID, bool) {
	rawID := c.Param("id")
	if rawID == "" {
		sloggin.Get(c).Warn("missing circle id parameter")
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing circle id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		sloggin.Get(c).Warn("invalid circle id format", "id", rawID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid circle id"})
		return 0, false
	}

	return circle.ID(parsed), true
}

func parseContestID(c *gin.Context) (contest.ID, bool) {
	rawID := c.Param("contestID")
	if rawID == "" {
		sloggin.Get(c).Warn("missing contest id parameter")
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing contest id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		sloggin.Get(c).Warn("invalid contest id format", "id", rawID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contest id"})
		return 0, false
	}

	return contest.ID(parsed), true
}

func parseContestIDFromRoute(c *gin.Context) (contest.ID, bool) {
	rawID := c.Param("contestID")
	if rawID == "" {
		sloggin.Get(c).Warn("missing contest id parameter")
		c.JSON(http.StatusBadRequest, gin.H{"error": "missing contest id"})
		return 0, false
	}

	parsed, err := strconv.ParseInt(rawID, 10, 32)
	if err != nil {
		sloggin.Get(c).Warn("invalid contest id format", "id", rawID, "error", err)
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid contest id"})
		return 0, false
	}

	return contest.ID(parsed), true
}

func toEnrichedCircleResponse(enriched *service.EnrichedCircle) circleResponse {
	members := make([]memberResponse, len(enriched.Members))
	for i, m := range enriched.Members {
		members[i] = memberResponse{
			UserID:   uuid.UUID(m.UserID).String(),
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

func toContestResponse(cont *contest.Contest) contestResponse {
	options := make([]optionResponse, 0, len(cont.Options))
	for _, opt := range cont.Options {
		options = append(options, optionResponse{ID: opt.ID, Text: opt.Text})
	}
	sort.Slice(options, func(i, j int) bool {
		return options[i].ID < options[j].ID
	})

	predictions := make([]predictionResponse, len(cont.Predictions))
	for i, pred := range cont.Predictions {
		predictions[i] = predictionResponse{
			UserID:    uuid.UUID(pred.UserID).String(),
			OptionID:  pred.OptionID,
			Clout:     pred.Clout,
			Timestamp: pred.Timestamp,
		}
	}

	return contestResponse{
		ID:             int32(cont.ID),
		CircleID:       int32(cont.CircleID),
		CreatorID:      uuid.UUID(cont.CreatorID).String(),
		Question:       cont.Question,
		Options:        options,
		Predictions:    predictions,
		Status:         string(cont.Status),
		MinStake:       cont.MinStake,
		TotalPot:       cont.CalculatePot(),
		HouseRake:      cont.CalculateHouseRakeClout(),
		ResultOptionID: cont.ResultOptionID,
		CreatedAt:      cont.CreatedAt,
		LockedAt:       cont.LockedAt,
		Duration:       cont.Duration,
	}
}

func (h *Handler) streamContestEvents(c *gin.Context) {
	circleID, ok := parseCircleID(c)
	if !ok {
		return
	}

	contestID, ok := parseContestIDFromRoute(c)
	if !ok {
		return
	}

	// Verify contest belongs to circle
	_, err := h.svc.GetContestInCircle(c.Request.Context(), circleID, contestID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || errors.Is(err, service.ErrContestNotFound) {
			sloggin.Get(c).Warn("contest not found for streaming", "contest_id", contestID)
			c.JSON(http.StatusNotFound, gin.H{"error": "contest not found"})
			return
		}
		sloggin.Get(c).Error("failed to get contest for streaming", "contest_id", contestID, "error", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	// Subscribe to events
	events, unsubscribe := h.subscriber.Subscribe(contestID)
	defer unsubscribe()

	sloggin.Get(c).Info("client subscribed to contest events", "contest_id", contestID)

	init := make(chan struct{})

	go func() {
		select {
		case <-c.Request.Context().Done():
			sloggin.Get(c).Info("stopping initial event sender due to client disconnect", "contest_id", contestID)
			return
		case init <- struct{}{}:
		}
	}()

	c.Stream(func(w io.Writer) bool {
		select {
		case <-c.Request.Context().Done():
			sloggin.Get(c).Info("client disconnected from contest event stream", "contest_id", contestID)
			return false
		case event := <-events:
			c.SSEvent(
				"contest_updated",
				event,
			)
			return true

		case <-init:
			c.SSEvent("init", "connected")
			return true
		}
	})
}

func sseHeadersMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Header("Content-Type", "text/event-stream")
		c.Header("Cache-Control", "no-cache")
		c.Header("Connection", "keep-alive")
		c.Header("Transfer-Encoding", "chunked")
		c.Next()
	}
}
