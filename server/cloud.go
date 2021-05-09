// SPDX-FileCopyrightText: 2021 Softbear, Inc.
// SPDX-License-Identifier: AGPL-3.0-or-later

package main

import (
	"fmt"
)

func (h *Hub) Cloud() {
	fmt.Println("Updating cloud")

	playerCount := 0

	// Note: Cannot use to determine number of players, as long as there
	// are duplicate names
	playerScores := make(map[string]int)

	for client := h.clients.First; client != nil; client = client.Data().Next {
		if _, ok := client.(*SocketClient); ok {
			playerCount++
			player := &client.Data().Player
			if player.Score > 0 {
				playerScores[player.Name] = player.Score
			}
		}
	}

	go func() {
		err := h.cloud.UpdateLeaderboard(playerScores)
		if err != nil {
			fmt.Println("leaderboard error:", err)
		}
	}()

	statusJSON, err := json.Marshal(struct {
		Players int `json:"clients"`
	}{
		Players: playerCount,
	})

	if err == nil {
		h.statusJSON.Store(statusJSON)
	} else {
		fmt.Println("error marshaling status:", err)
	}

	_ = h.cloud.UpdateServer(playerCount)
}