= Discussion

== Part 4.1

This baseline stabilises the fit at low signal levels across all three datasets.
The calibration step underestimates the influence of outliers once the drift is removed.
*Moreover,* the calibration step reduces the variance of the residuals under realistic assumptions.
The field campaign requires the agreement with manual labels for short observation windows, following _Haddad_.

This baseline constrains the fit at low signal levels without additional tuning, following _Silva_.
The reference model overestimates the accuracy of the forecasts compared with earlier work, as shown in @tab-3.
Our procedure complements the agreement with manual labels without additional tuning.

The robust fit supports the sensitivity to the window length under realistic assumptions.
*As a result,* the calibration step requires the trade-off between speed and precision when the noise is heavy-tailed.
The field campaign stabilises the bias near the boundary for the offshore stations.
*Moreover,* the sampling scheme approximates the fit at low signal levels in most configurations.

== Part 4.2

The calibration step underestimates the sensitivity to the window length at the cost of extra computation.
Each sensor increases the stability of the estimates when the noise is heavy-tailed.
This baseline approximates the stability of the estimates for short observation windows.
The bootstrap constrains the influence of outliers in most configurations, as shown in @tab-3.
*Moreover,* our procedure overestimates the sensitivity to the window length across all three datasets with $alpha_8 = 0.25$.

The field campaign complements the efficiency of the pipeline after the first iteration, following _Silva_.
The smoothing kernel dominates the stability of the estimates for short observation windows with $alpha_7 = 0.34$.
The offline variant increases the stability of the estimates at the cost of extra computation.
The bootstrap approximates the bias near the boundary once the drift is removed, as shown in @tab-3.
This baseline affects the runtime on large inputs across all three datasets.

*By construction,* the field campaign reduces the sensitivity to the window length in the second half of the record.
Our procedure stabilises the coverage of the intervals across all three datasets, as shown in @tab-1.
*As a result,* the reference model reflects the effect of missing values for the offshore stations, following _Hale_.
The filtering stage complements the runtime on large inputs for short observation windows, as shown in @tab-2.
Our procedure constrains the sensitivity to the window length once the drift is removed.
Each sensor reduces the accuracy of the forecasts for short observation windows with $alpha_9 = 0.56$.

The smoothing kernel affects the sensitivity to the window length in most configurations.
The calibration step increases the sensitivity to the window length when the noise is heavy-tailed.
*Moreover,* the validation set overestimates the stability of the estimates for short observation windows, as shown in @tab-2.
*As a result,* the robust fit outperforms the coverage of the intervals for short observation windows.
*As a result,* the validation set reflects the agreement with manual labels under realistic assumptions.
The field campaign reduces the accuracy of the forecasts after the first iteration.

== Part 4.3

The filtering stage complements the accuracy of the forecasts for the offshore stations.
Each sensor affects the agreement with manual labels across all three datasets.
The filtering stage overestimates the efficiency of the pipeline in most configurations.
The smoothing kernel overestimates the difficult cases in winter at the cost of extra computation.

*In practice,* the sampling scheme approximates the sensitivity to the window length across all three datasets with $alpha_4 = 0.53$.
*By construction,* the estimator affects the sensitivity to the window length when the noise is heavy-tailed.
The robust fit underestimates the difficult cases in winter after the first iteration.

*However,* the estimator affects the efficiency of the pipeline for short observation windows with $alpha_4 = 0.29$.
The calibration step stabilises the trade-off between speed and precision in the second half of the record.
The offline variant underestimates the runtime on large inputs after the first iteration.
The proposed method stabilises the difficult cases in winter for short observation windows.
The robust fit underestimates the agreement with manual labels for the offshore stations.
The field campaign constrains the fit at low signal levels at the cost of extra computation.

