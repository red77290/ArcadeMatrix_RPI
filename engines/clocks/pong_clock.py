from PIL import Image, ImageDraw, ImageFont
from datetime import datetime
import random
import math

class PongClock:
    def __init__(self, width, height):
        self.w = width
        self.h = height
        self.ball_x = width // 2
        self.ball_y = height // 2
        self.ball_dx = 2.0
        self.ball_dy = 1.5
        self.ball_size = 2
        
        self.pad_h = 12
        self.pad_w = 2
        self.p1_y = (height - self.pad_h) // 2
        self.p2_y = (height - self.pad_h) // 2
        
        self.last_minute = -1
        self.force_miss = False
        
    def reset_ball(self, left_served):
        self.ball_y = self.h // 2
        self.ball_dy = random.uniform(-1.5, 1.5)
        if left_served:
            self.ball_x = self.pad_w + 1
            self.ball_dx = 2.0
        else:
            self.ball_x = self.w - self.pad_w - 3
            self.ball_dx = -2.0

    def tick(self, img, time_str, font, color1, color2):
        draw = ImageDraw.Draw(img)
        
        # Parse time
        parts = time_str.split(':')
        if len(parts) >= 2:
            score_left = parts[0]
            score_right = parts[1]
        else:
            score_left = "00"
            score_right = "00"
            
        now_min = int(score_right) if score_right.isdigit() else 0
        if self.last_minute == -1:
            self.last_minute = now_min
        elif self.last_minute != now_min:
            self.force_miss = True
            self.last_minute = now_min
            
        # Draw dotted middle line
        for y in range(0, self.h, 4):
            draw.line([(self.w//2, y), (self.w//2, y+1)], fill=(100, 100, 100))
            
        # Draw scores
        try:
            bbox_l = draw.textbbox((0, 0), score_left, font=font)
            w_l = bbox_l[2] - bbox_l[0]
        except:
            w_l = 15
            
        draw.text(((self.w//2) - w_l - 8, 4), score_left, font=font, fill=color1)
        draw.text(((self.w//2) + 8, 4), score_right, font=font, fill=color2)
        
        # Physics
        self.ball_x += self.ball_dx
        self.ball_y += self.ball_dy
        
        # Top/Bottom Bounce
        if self.ball_y <= 0:
            self.ball_y = 0
            self.ball_dy *= -1
        elif self.ball_y >= self.h - self.ball_size:
            self.ball_y = self.h - self.ball_size
            self.ball_dy *= -1
            
        # P1 AI (Left) - Perfect tracking
        target_p1 = self.ball_y - (self.pad_h // 2)
        if self.ball_dx < 0:
            if self.p1_y < target_p1: self.p1_y += 1.5
            if self.p1_y > target_p1: self.p1_y -= 1.5
            
        # P2 AI (Right) - Perfect unless force_miss
        target_p2 = self.ball_y - (self.pad_h // 2)
        if self.ball_dx > 0:
            if self.force_miss:
                # Move away from ball
                if self.ball_y > self.h // 2: target_p2 = 0
                else: target_p2 = self.h - self.pad_h
                
            if self.p2_y < target_p2: self.p2_y += 1.5
            if self.p2_y > target_p2: self.p2_y -= 1.5
            
        # Clamp paddles
        self.p1_y = max(0, min(self.h - self.pad_h, self.p1_y))
        self.p2_y = max(0, min(self.h - self.pad_h, self.p2_y))
        
        # Paddle collisions
        if self.ball_dx < 0 and self.ball_x <= self.pad_w:
            if self.p1_y - self.ball_size <= self.ball_y <= self.p1_y + self.pad_h:
                self.ball_x = self.pad_w
                self.ball_dx *= -1.05  # Slight speedup
                self.ball_dy += (self.ball_y - (self.p1_y + self.pad_h/2)) * 0.2
        elif self.ball_dx > 0 and self.ball_x >= self.w - self.pad_w - self.ball_size:
            if self.p2_y - self.ball_size <= self.ball_y <= self.p2_y + self.pad_h:
                self.ball_x = self.w - self.pad_w - self.ball_size
                self.ball_dx *= -1.05
                self.ball_dy += (self.ball_y - (self.p2_y + self.pad_h/2)) * 0.2
                
        # Speed cap
        self.ball_dx = max(-4.0, min(4.0, self.ball_dx))
        self.ball_dy = max(-3.0, min(3.0, self.ball_dy))
        
        # Scoring (Out of bounds)
        if self.ball_x < -10:
            self.reset_ball(left_served=True)
            self.force_miss = False
        elif self.ball_x > self.w + 10:
            self.reset_ball(left_served=False)
            self.force_miss = False
            
        # Draw paddles
        draw.rectangle([0, int(self.p1_y), self.pad_w - 1, int(self.p1_y) + self.pad_h], fill=(255, 255, 255))
        draw.rectangle([self.w - self.pad_w, int(self.p2_y), self.w - 1, int(self.p2_y) + self.pad_h], fill=(255, 255, 255))
        
        # Draw ball
        draw.rectangle([int(self.ball_x), int(self.ball_y), int(self.ball_x) + self.ball_size - 1, int(self.ball_y) + self.ball_size - 1], fill=(255, 255, 255))
        
        return img
