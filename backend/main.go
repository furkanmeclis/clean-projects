package main

import (
	"crypto/sha1"
	"embed"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

//go:embed static/*
var staticFiles embed.FS

type Candidate struct {
	ID          string `json:"id"`
	Label       string `json:"label"`
	Path        string `json:"path"`
	Kind        string `json:"kind"`
	Method      string `json:"method"`
	SizeBytes   int64  `json:"sizeBytes"`
	SizeHuman   string `json:"sizeHuman"`
	Modified    string `json:"modified"`
	Available   bool   `json:"available"`
	Description string `json:"description"`
	Risk        string `json:"risk"`
	Excluded    bool   `json:"excluded"`
}

type ScanResponse struct {
	Disk        DiskInfo    `json:"disk"`
	Candidates  []Candidate `json:"candidates"`
	Protected   []string    `json:"protected"`
	GeneratedAt string      `json:"generatedAt"`
	State       AppState    `json:"state"`
}

type DiskInfo struct {
	Used  string `json:"used"`
	Free  string `json:"free"`
	Total string `json:"total"`
	Pct   string `json:"pct"`
}

type DeleteRequest struct {
	IDs []string `json:"ids"`
}

type DeleteResult struct {
	ID     string `json:"id"`
	Label  string `json:"label"`
	OK     bool   `json:"ok"`
	Error  string `json:"error,omitempty"`
	Before string `json:"before"`
}

type AppState struct {
	ExcludedIDs map[string]bool `json:"excludedIds"`
	History     []HistoryEntry  `json:"history"`
}

type HistoryEntry struct {
	Time       string        `json:"time"`
	TotalBytes int64         `json:"totalBytes"`
	TotalHuman string        `json:"totalHuman"`
	Items      []HistoryItem `json:"items"`
}

type HistoryItem struct {
	ID        string `json:"id"`
	Label     string `json:"label"`
	Path      string `json:"path"`
	Kind      string `json:"kind"`
	SizeBytes int64  `json:"sizeBytes"`
	SizeHuman string `json:"sizeHuman"`
}

type ExclusionRequest struct {
	ID       string `json:"id"`
	Excluded bool   `json:"excluded"`
}

type CleanupJob struct {
	ID      string     `json:"id"`
	Events  []JobEvent `json:"events"`
	Done    bool       `json:"done"`
	Success bool       `json:"success"`
	mu      sync.Mutex
	cond    *sync.Cond
}

type JobEvent struct {
	Time    string `json:"time"`
	Type    string `json:"type"`
	Message string `json:"message"`
	Label   string `json:"label,omitempty"`
	Size    string `json:"size,omitempty"`
}

var (
	homeDir, _       = os.UserHomeDir()
	documentsDir     = filepath.Join(homeDir, "Documents")
	icloudMobileDocs = filepath.Join(homeDir, "Library", "Mobile Documents")
	protectedRoots   = []string{
		icloudMobileDocs,
		filepath.Join(homeDir, "Library", "CloudStorage"),
		filepath.Join(homeDir, "Library", "Mobile Documents"),
	}
	projectNames = map[string]bool{
		"node_modules": true, ".next": true, ".nuxt": true, ".turbo": true,
		".expo": true, ".tamagui": true, "target": true, ".venv": true, "venv": true,
	}
	jobsMu sync.Mutex
	jobs   = map[string]*CleanupJob{}

	scanMu   sync.Mutex
	lastScan ScanResponse
	hasScan  bool
)

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/scan", handleScan)
	mux.HandleFunc("/api/delete", handleDelete)
	mux.HandleFunc("/api/exclusion", handleExclusion)
	mux.HandleFunc("/api/jobs/", handleJobEvents)
	mux.HandleFunc("/api/shutdown", handleShutdown)
	mux.HandleFunc("/api/state", handleState)
	mux.HandleFunc("/", handleStatic)

	addr := os.Getenv("CLEAN_PROJECTS_ADDR")
	if addr == "" {
		addr = "127.0.0.1:51392"
	}
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		log.Fatal(err)
	}
	url := "http://" + ln.Addr().String()
	fmt.Println("CleanProjects web UI:", url)
	if os.Getenv("CLEAN_PROJECTS_NO_OPEN") != "1" {
		openBrowser(url)
	}
	log.Fatal(http.Serve(ln, mux))
}

func handleScan(w http.ResponseWriter, r *http.Request) {
	result := scan()
	setCachedScan(result)
	writeJSON(w, result)
}

func handleState(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, loadState())
}

func handleDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req DeleteRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	current, ok := getCachedScan()
	if !ok {
		current = scan()
		setCachedScan(current)
	}
	byID := map[string]Candidate{}
	for _, c := range current.Candidates {
		byID[c.ID] = c
	}

	targets := []Candidate{}
	for _, id := range req.IDs {
		c, ok := byID[id]
		if ok && c.Available {
			targets = append(targets, c)
		}
	}

	job := newCleanupJob()
	registerJob(job)
	go runCleanupJob(job, targets)

	writeJSON(w, map[string]any{"jobId": job.ID})
}

func handleExclusion(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var req ExclusionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	state := loadState()
	if state.ExcludedIDs == nil {
		state.ExcludedIDs = map[string]bool{}
	}
	if req.Excluded {
		state.ExcludedIDs[req.ID] = true
	} else {
		delete(state.ExcludedIDs, req.ID)
	}
	if err := saveState(state); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	writeJSON(w, map[string]any{"state": state, "scan": scan()})
}

func handleShutdown(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	writeJSON(w, map[string]any{"ok": true, "message": "CleanProjects is shutting down"})
	go func() {
		time.Sleep(250 * time.Millisecond)
		os.Exit(0)
	}()
}

func handleJobEvents(w http.ResponseWriter, r *http.Request) {
	if !strings.HasSuffix(r.URL.Path, "/events") {
		http.NotFound(w, r)
		return
	}
	id := strings.TrimSuffix(strings.TrimPrefix(r.URL.Path, "/api/jobs/"), "/events")
	job := getJob(id)
	if job == nil {
		http.NotFound(w, r)
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "streaming unsupported", http.StatusInternalServerError)
		return
	}

	index := 0
	for {
		job.mu.Lock()
		for index >= len(job.Events) && !job.Done {
			job.cond.Wait()
		}
		for index < len(job.Events) {
			writeSSE(w, job.Events[index])
			index++
		}
		done := job.Done
		job.mu.Unlock()
		flusher.Flush()
		if done {
			return
		}
	}
}

func handleStatic(w http.ResponseWriter, r *http.Request) {
	sub, _ := fs.Sub(staticFiles, "static")
	if _, err := fs.Stat(sub, strings.TrimPrefix(r.URL.Path, "/")); err != nil && r.URL.Path != "/" {
		r.URL.Path = "/"
	}
	http.FileServer(http.FS(sub)).ServeHTTP(w, r)
}

func scan() ScanResponse {
	state := loadState()
	candidates := []Candidate{}
	candidates = append(candidates, scanProjectArtifacts(documentsDir)...)
	candidates = append(candidates, globalCandidates()...)
	candidates = append(candidates, commandCandidates()...)
	for i := range candidates {
		candidates[i].Excluded = state.ExcludedIDs[candidates[i].ID]
	}
	sort.Slice(candidates, func(i, j int) bool {
		if candidates[i].Available != candidates[j].Available {
			return candidates[i].Available
		}
		return candidates[i].SizeBytes > candidates[j].SizeBytes
	})
	return ScanResponse{
		Disk:        diskInfo(),
		Candidates:  candidates,
		Protected:   protectedRoots,
		GeneratedAt: time.Now().Format(time.RFC3339),
		State:       state,
	}
}

func setCachedScan(result ScanResponse) {
	scanMu.Lock()
	defer scanMu.Unlock()
	lastScan = result
	hasScan = true
}

func getCachedScan() (ScanResponse, bool) {
	scanMu.Lock()
	defer scanMu.Unlock()
	return lastScan, hasScan
}

func statePath() string {
	return filepath.Join(filepath.Dir(os.Args[0]), "clean-projects-state.json")
}

func loadState() AppState {
	state := AppState{ExcludedIDs: map[string]bool{}, History: []HistoryEntry{}}
	data, err := os.ReadFile(statePath())
	if err != nil {
		return state
	}
	if err := json.Unmarshal(data, &state); err != nil {
		return AppState{ExcludedIDs: map[string]bool{}, History: []HistoryEntry{}}
	}
	if state.ExcludedIDs == nil {
		state.ExcludedIDs = map[string]bool{}
	}
	if state.History == nil {
		state.History = []HistoryEntry{}
	}
	return state
}

func saveState(state AppState) error {
	data, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(statePath(), data, 0o600)
}

func appendHistory(entry HistoryEntry) {
	state := loadState()
	state.History = append([]HistoryEntry{entry}, state.History...)
	if len(state.History) > 100 {
		state.History = state.History[:100]
	}
	_ = saveState(state)
}

func newCleanupJob() *CleanupJob {
	job := &CleanupJob{ID: idFor(fmt.Sprintf("%d", time.Now().UnixNano()))}
	job.cond = sync.NewCond(&job.mu)
	return job
}

func registerJob(job *CleanupJob) {
	jobsMu.Lock()
	defer jobsMu.Unlock()
	jobs[job.ID] = job
}

func getJob(id string) *CleanupJob {
	jobsMu.Lock()
	defer jobsMu.Unlock()
	return jobs[id]
}

func (job *CleanupJob) log(eventType, message string, parts ...string) {
	event := JobEvent{
		Time:    time.Now().Format("15:04:05"),
		Type:    eventType,
		Message: message,
	}
	if len(parts) > 0 {
		event.Label = parts[0]
	}
	if len(parts) > 1 {
		event.Size = parts[1]
	}
	job.mu.Lock()
	job.Events = append(job.Events, event)
	job.mu.Unlock()
	job.cond.Broadcast()
}

func (job *CleanupJob) finish(success bool) {
	job.mu.Lock()
	job.Done = true
	job.Success = success
	job.mu.Unlock()
	job.cond.Broadcast()
}

func writeSSE(w http.ResponseWriter, event JobEvent) {
	data, _ := json.Marshal(event)
	fmt.Fprintf(w, "event: %s\n", event.Type)
	fmt.Fprintf(w, "data: %s\n\n", data)
}

func runCleanupJob(job *CleanupJob, targets []Candidate) {
	job.log("info", fmt.Sprintf("Cleanup job %s started", job.ID))
	job.log("info", fmt.Sprintf("%d item(s) queued", len(targets)))
	if len(targets) == 0 {
		job.log("done", "Nothing selected")
		job.finish(true)
		return
	}

	historyItems := []HistoryItem{}
	var totalDeleted int64
	success := true

	for _, candidate := range targets {
		job.log("start", "Starting", candidate.Label, candidate.SizeHuman)
		if err := deleteCandidateWithLog(candidate, func(message string) {
			job.log("output", message, candidate.Label)
		}); err != nil {
			success = false
			job.log("error", err.Error(), candidate.Label)
			continue
		}
		totalDeleted += candidate.SizeBytes
		historyItems = append(historyItems, HistoryItem{
			ID: candidate.ID, Label: candidate.Label, Path: candidate.Path, Kind: candidate.Kind,
			SizeBytes: candidate.SizeBytes, SizeHuman: candidate.SizeHuman,
		})
		job.log("success", "Completed", candidate.Label, candidate.SizeHuman)
	}

	if len(historyItems) > 0 {
		appendHistory(HistoryEntry{
			Time:       time.Now().Format(time.RFC3339),
			TotalBytes: totalDeleted,
			TotalHuman: human(totalDeleted),
			Items:      historyItems,
		})
	}
	job.log("done", fmt.Sprintf("Cleanup finished. Estimated reclaimed: %s", human(totalDeleted)))
	job.finish(success)
}

func scanProjectArtifacts(root string) []Candidate {
	var out []Candidate
	filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil || !d.IsDir() || isProtected(path) {
			if d != nil && d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		name := d.Name()
		if name == ".git" || name == "Backups" {
			return filepath.SkipDir
		}
		if projectNames[name] || isBuildArtifact(path) || isComposerVendor(path) {
			if safeProjectPath(path) {
				out = append(out, makePathCandidate(name, path, "project", "remove", "Regenerable project dependency or build output.", "low"))
			}
			return filepath.SkipDir
		}
		return nil
	})
	return filterSmall(out, 1024*1024)
}

func globalCandidates() []Candidate {
	specs := []struct {
		label, path, kind, method, desc, risk string
	}{
		{"Xcode DerivedData", filepath.Join(homeDir, "Library/Developer/Xcode/DerivedData"), "xcode", "remove", "Xcode build cache. Projects rebuild it.", "low"},
		{"Xcode device logs", filepath.Join(homeDir, "Library/Developer/Xcode/DeviceLogs"), "xcode", "remove", "Device logs captured by Xcode.", "low"},
		{"Xcode iOS DeviceSupport", filepath.Join(homeDir, "Library/Developer/Xcode/iOS DeviceSupport"), "xcode", "remove", "Symbols/support files for connected iOS devices.", "medium"},
		{"Xcode Products cache", filepath.Join(homeDir, "Library/Developer/Xcode/Products"), "xcode", "remove", "Cached installed product metadata.", "low"},
		{"Android SDK", filepath.Join(homeDir, "Library/Android/sdk"), "mobile", "remove", "Android SDK, emulator images and NDK. Reinstall through Android Studio/sdkmanager.", "high"},
		{"pnpm store", filepath.Join(homeDir, "Library/pnpm/store"), "cache", "pnpm", "pnpm package store. Recreated on install.", "low"},
		{"uv cache", filepath.Join(homeDir, ".cache/uv"), "cache", "remove", "Python uv cache.", "low"},
		{"Codex runtime cache", filepath.Join(homeDir, ".cache/codex-runtimes"), "cache", "remove", "Bundled runtime dependency cache.", "medium"},
		{"Spotify cache", filepath.Join(homeDir, "Library/Caches/com.spotify.client"), "cache", "remove", "Spotify media cache.", "low"},
		{"Google Chrome cache", filepath.Join(homeDir, "Library/Caches/Google"), "cache", "remove", "Chrome cache files.", "low"},
		{"Chrome on-device model", filepath.Join(homeDir, "Library/Application Support/Google/Chrome/OptGuideOnDeviceModel"), "cache", "remove", "Chrome local optimization model cache.", "low"},
		{"Cursor cached app data", filepath.Join(homeDir, "Library/Application Support/Cursor/CachedData"), "cache", "remove", "Old Cursor Electron app caches.", "low"},
		{"Cursor extension trash", filepath.Join(homeDir, "Library/Application Support/Cursor/CachedExtensionVSIXs/.trash"), "cache", "remove", "Cursor cached extension trash.", "low"},
		{"Cursor global state DB", filepath.Join(homeDir, "Library/Application Support/Cursor/User/globalStorage/state.vscdb"), "app-data", "remove", "Large Cursor global state database. Close Cursor before deleting; settings/state can reset.", "high"},
		{"Claude VM bundle", filepath.Join(homeDir, "Library/Application Support/Claude/vm_bundles"), "app-data", "remove", "Claude local VM bundle. Close Claude before deleting; it may redownload.", "high"},
	}

	out := make([]Candidate, 0, len(specs))
	for _, s := range specs {
		out = append(out, makePathCandidate(s.label, s.path, s.kind, s.method, s.desc, s.risk))
	}
	return out
}

func commandCandidates() []Candidate {
	out := []Candidate{
		makeCommandCandidate("Docker build cache prune", "docker-build-prune", dockerReclaimable("Build Cache"), "docker builder prune -af", "Prunes Docker build cache only.", "medium"),
		makeCommandCandidate("Docker unused images/containers prune", "docker-system-prune", dockerReclaimable("Images")+dockerReclaimable("Containers")+dockerReclaimable("Local Volumes"), "docker system prune -af", "Prunes unused Docker images and stopped containers. Volumes are not removed.", "medium"),
	}
	out = append(out, scanSimulatorRuntimes()...)
	return out
}

func scanSimulatorRuntimes() []Candidate {
	output, err := exec.Command("xcrun", "simctl", "runtime", "list").Output()
	if err != nil {
		return []Candidate{makeCommandCandidate("iOS Simulator runtimes", "sim-runtime-none", 0, "simctl", "No simulator runtime found or simctl unavailable.", "low")}
	}
	lines := strings.Split(string(output), "\n")
	var out []Candidate
	for _, line := range lines {
		if !strings.Contains(line, " - ") || !strings.Contains(line, "(") {
			continue
		}
		parts := strings.Split(line, " - ")
		if len(parts) < 2 {
			continue
		}
		idPart := strings.Fields(parts[len(parts)-1])
		if len(idPart) == 0 {
			continue
		}
		id := idPart[0]
		label := strings.TrimSpace(parts[0])
		size := int64(0)
		if strings.Contains(line, "Deleting") {
			label += " (deleting)"
		}
		out = append(out, makeCommandCandidate("iOS Simulator runtime: "+label, "sim-runtime-"+id, size, "xcrun simctl runtime delete "+id, "Installed iOS simulator runtime. Reinstall from Xcode when needed.", "high"))
	}
	if len(out) == 0 {
		out = append(out, makeCommandCandidate("iOS Simulator runtimes", "sim-runtime-none", 0, "simctl", "No installed simulator runtime found.", "low"))
	}
	return out
}

func deleteCandidate(c Candidate) error {
	return deleteCandidateWithLog(c, func(string) {})
}

func deleteCandidateWithLog(c Candidate, logf func(string)) error {
	if c.Method == "remove" {
		if c.Path == "" || isProtected(c.Path) {
			return errors.New("protected path")
		}
		logf("remove " + c.Path)
		return os.RemoveAll(c.Path)
	}
	switch {
	case c.Method == "pnpm":
		if _, err := exec.LookPath("pnpm"); err == nil {
			return runCommandWithLog(logf, "pnpm", "store", "prune")
		}
		logf("pnpm not found; removing store path directly")
		return os.RemoveAll(c.Path)
	case strings.HasPrefix(c.Method, "docker builder prune"):
		return runCommandWithLog(logf, "docker", "builder", "prune", "-af")
	case strings.HasPrefix(c.Method, "docker system prune"):
		return runCommandWithLog(logf, "docker", "system", "prune", "-af")
	case strings.HasPrefix(c.Method, "xcrun simctl runtime delete "):
		fields := strings.Fields(c.Method)
		return runCommandWithLog(logf, "xcrun", "simctl", "runtime", "delete", fields[len(fields)-1])
	}
	return errors.New("unsupported delete method")
}

func runCommandWithLog(logf func(string), name string, args ...string) error {
	logf("$ " + strings.Join(append([]string{name}, args...), " "))
	cmd := exec.Command(name, args...)
	output, err := cmd.CombinedOutput()
	for _, line := range strings.Split(strings.TrimSpace(string(output)), "\n") {
		if strings.TrimSpace(line) != "" {
			logf(line)
		}
	}
	return err
}

func makePathCandidate(label, path, kind, method, desc, risk string) Candidate {
	size, mod, available := pathStats(path)
	return Candidate{
		ID: idFor(method + "|" + path), Label: label, Path: path, Kind: kind, Method: method,
		SizeBytes: size, SizeHuman: human(size), Modified: mod, Available: available,
		Description: desc, Risk: risk,
	}
}

func makeCommandCandidate(label, seed string, size int64, method, desc, risk string) Candidate {
	return Candidate{
		ID: idFor(seed + "|" + method), Label: label, Kind: "command", Method: method,
		SizeBytes: size, SizeHuman: human(size), Available: size > 0 || strings.Contains(method, "runtime delete"),
		Description: desc, Risk: risk,
	}
}

func isBuildArtifact(path string) bool {
	base := filepath.Base(path)
	clean := filepath.ToSlash(path)
	return base == "dist" || strings.HasSuffix(clean, "/public/build") || strings.HasSuffix(clean, "/frontend/dist") || strings.Contains(clean, "/public/themes/") && strings.HasSuffix(clean, "/build")
}

func isComposerVendor(path string) bool {
	if filepath.Base(path) != "vendor" {
		return false
	}
	clean := filepath.ToSlash(path)
	return !strings.Contains(clean, "/public/vendor") && !strings.Contains(clean, "/lang/vendor") && !strings.Contains(clean, "/resources/views/vendor") && !strings.Contains(clean, "/js/vendor")
}

func safeProjectPath(path string) bool {
	clean := filepath.ToSlash(path)
	return !strings.Contains(clean, "/.git/") && !strings.Contains(clean, "/Backups/") && !strings.Contains(clean, "/Library/Mobile Documents/")
}

func filterSmall(in []Candidate, min int64) []Candidate {
	out := in[:0]
	for _, c := range in {
		if c.SizeBytes >= min && c.Available {
			out = append(out, c)
		}
	}
	return out
}

func isProtected(path string) bool {
	abs, _ := filepath.Abs(path)
	for _, root := range protectedRoots {
		rabs, _ := filepath.Abs(root)
		if abs == rabs || strings.HasPrefix(abs, rabs+string(os.PathSeparator)) {
			return true
		}
	}
	return false
}

func pathStats(path string) (int64, string, bool) {
	info, err := os.Stat(path)
	if err != nil {
		return 0, "", false
	}
	size := dirSize(path)
	return size, info.ModTime().Format("2006-01-02 15:04"), true
}

func dirSize(path string) int64 {
	var total int64
	filepath.WalkDir(path, func(p string, d fs.DirEntry, err error) error {
		if err != nil || isProtected(p) {
			if d != nil && d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if info, err := d.Info(); err == nil {
			total += info.Size()
		}
		return nil
	})
	return total
}

func dockerReclaimable(kind string) int64 {
	out, err := exec.Command("docker", "system", "df").Output()
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(out), "\n") {
		if strings.HasPrefix(line, kind) {
			fields := strings.Fields(line)
			if len(fields) >= 5 {
				return parseHuman(fields[4])
			}
		}
	}
	return 0
}

func diskInfo() DiskInfo {
	out, err := exec.Command("df", "-h", "/System/Volumes/Data").Output()
	if err != nil {
		return DiskInfo{}
	}
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	if len(lines) < 2 {
		return DiskInfo{}
	}
	f := strings.Fields(lines[1])
	if len(f) < 5 {
		return DiskInfo{}
	}
	return DiskInfo{Total: f[1], Used: f[2], Free: f[3], Pct: f[4]}
}

func human(n int64) string {
	if n <= 0 {
		return "0B"
	}
	units := []string{"B", "KB", "MB", "GB", "TB"}
	v := float64(n)
	i := 0
	for v >= 1024 && i < len(units)-1 {
		v /= 1024
		i++
	}
	return fmt.Sprintf("%.1f%s", v, units[i])
}

func parseHuman(s string) int64 {
	s = strings.TrimSpace(strings.TrimRight(s, "B"))
	mult := float64(1)
	if strings.HasSuffix(s, "K") {
		mult = 1024
		s = strings.TrimSuffix(s, "K")
	} else if strings.HasSuffix(s, "M") {
		mult = 1024 * 1024
		s = strings.TrimSuffix(s, "M")
	} else if strings.HasSuffix(s, "G") {
		mult = 1024 * 1024 * 1024
		s = strings.TrimSuffix(s, "G")
	}
	v, _ := strconv.ParseFloat(s, 64)
	return int64(v * mult)
}

func idFor(s string) string {
	sum := sha1.Sum([]byte(s))
	return hex.EncodeToString(sum[:])[:16]
}

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(v)
}

func openBrowser(url string) {
	_ = exec.Command("open", url).Start()
}
